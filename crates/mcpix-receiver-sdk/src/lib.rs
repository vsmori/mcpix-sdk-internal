//! Fachada Rust do SDK do recebedor.
//!
//! Compõe o núcleo (`mcpix-core`) com implementações concretas de `SeedStore`
//! e `Counter`. Os bindings nativos (Swift, Kotlin, .NET) consomem este crate
//! através de `mcpix-ffi`.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]

pub mod integrity_runtime;
pub mod memory_store;
pub mod monotonic_counter;
pub mod system_random;

#[cfg(feature = "sqlite")]
pub mod sqlite_store;

use std::sync::Arc;

use mcpix_core::error::McpixError;
use mcpix_core::state::{
    apply_generate_charge, apply_validate_receipt, GenerateChargeCommand, ValidationOutcome,
};
use mcpix_core::traits::{Counter, SecureRandom, SeedStore};
use mcpix_core::types::{Charge, ConfirmationCode, RetainedReceipt, Seed, SeedId, SEED_LEN};

pub use mcpix_core::types;

/// Comprovante de registro emitido para um recebedor após `register`. O conteúdo
/// público é apenas o `SeedId`; o material da semente fica no `SeedStore`.
#[derive(Clone, Debug)]
pub struct RegistrationProof {
    pub seed_id: SeedId,
}

/// SDK do recebedor. Vida útil é a do dispositivo/aplicação.
pub struct ReceiverSdk {
    store: Arc<dyn SeedStore>,
    counter: Arc<dyn Counter>,
    rng: Arc<dyn SecureRandom>,
}

impl ReceiverSdk {
    pub fn new(
        store: Arc<dyn SeedStore>,
        counter: Arc<dyn Counter>,
        rng: Arc<dyn SecureRandom>,
    ) -> Self {
        Self { store, counter, rng }
    }

    /// Cadastra um recebedor: gera semente local e grava no store.
    ///
    /// Em produção, a semente é gerada e custodiada por Secure Enclave/HSM;
    /// aqui o material atravessa a memória, mas é zeroizado quando o `Seed`
    /// sai de escopo (ver `Seed: ZeroizeOnDrop`).
    pub fn register(&self, seed_id: SeedId) -> Result<RegistrationProof, McpixError> {
        let mut bytes = [0u8; SEED_LEN];
        self.rng.fill(&mut bytes)?;
        let seed = Seed::from_bytes(bytes);
        self.store.put_seed(&seed_id, seed)?;
        Ok(RegistrationProof { seed_id })
    }

    /// Gera uma cobrança nova: reserva contador, deriva (C₁, C₂), grava C₂
    /// retido localmente, devolve o campo de transporte público.
    pub fn generate_charge(
        &self,
        seed_id: &SeedId,
        amount_cents: u64,
    ) -> Result<Charge, McpixError> {
        let seed = self
            .store
            .get_seed(seed_id)?
            .ok_or(McpixError::UnknownSeed)?;
        let counter = self.counter.next(seed_id)?;
        let outcome = apply_generate_charge(
            &seed,
            GenerateChargeCommand {
                seed_id: seed_id.clone(),
                counter,
                amount_cents,
            },
        );
        self.store.save_receipt(outcome.retained)?;
        Ok(outcome.charge)
    }

    /// Valida um C₂ apresentado contra o retido. Em `Valid`, marca o registro
    /// como consumido antes de retornar — defesa de replay no nível do store.
    pub fn validate_receipt(
        &self,
        seed_id: &SeedId,
        counter: u64,
        presented_c2: &str,
    ) -> Result<ValidationOutcome, McpixError> {
        let presented = ConfirmationCode::parse(presented_c2)?;
        let retained = self
            .store
            .get_receipt(seed_id, counter)?
            .ok_or(McpixError::NoRetainedReceipt)?;
        let outcome = apply_validate_receipt(&retained, &presented);
        if matches!(outcome, ValidationOutcome::Valid) {
            self.store.mark_consumed(seed_id, counter)?;
        }
        Ok(outcome)
    }

    /// Permite inspecionar o registro retido — útil para a UI da demo.
    pub fn peek_retained(
        &self,
        seed_id: &SeedId,
        counter: u64,
    ) -> Result<Option<RetainedReceipt>, McpixError> {
        self.store.get_receipt(seed_id, counter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_store::InMemorySeedStore;
    use crate::monotonic_counter::InMemoryCounter;
    use crate::system_random::OsRandom;

    fn sdk() -> ReceiverSdk {
        ReceiverSdk::new(
            Arc::new(InMemorySeedStore::new()),
            Arc::new(InMemoryCounter::new()),
            Arc::new(OsRandom),
        )
    }

    #[test]
    fn full_local_flow() {
        let sdk = sdk();
        let proof = sdk.register(SeedId::new("R1").unwrap()).unwrap();
        let charge = sdk.generate_charge(&proof.seed_id, 9900).unwrap();
        let retained = sdk
            .peek_retained(&proof.seed_id, charge.counter)
            .unwrap()
            .unwrap();
        // No fluxo real C₂ chega via comprovante do pagador. Como ainda não
        // integramos o bank-payer-mock, simulamos apresentando o esperado.
        let outcome = sdk
            .validate_receipt(&proof.seed_id, charge.counter, retained.expected_c2.as_str())
            .unwrap();
        assert_eq!(outcome, ValidationOutcome::Valid);
    }

    #[test]
    fn replay_is_rejected() {
        let sdk = sdk();
        let proof = sdk.register(SeedId::new("R1").unwrap()).unwrap();
        let charge = sdk.generate_charge(&proof.seed_id, 100).unwrap();
        let retained = sdk
            .peek_retained(&proof.seed_id, charge.counter)
            .unwrap()
            .unwrap();
        let c2 = retained.expected_c2.as_str().to_string();
        assert_eq!(
            sdk.validate_receipt(&proof.seed_id, charge.counter, &c2).unwrap(),
            ValidationOutcome::Valid
        );
        assert_eq!(
            sdk.validate_receipt(&proof.seed_id, charge.counter, &c2).unwrap(),
            ValidationOutcome::Replay
        );
    }
}
