//! Camada UniFFI — superfície idiomática para Swift e Kotlin.
//!
//! Esta crate é a "tradutora" do `mcpix-receiver-sdk` (Rust) para tipos que
//! UniFFI sabe atravessar a fronteira. O scaffolding (`uniffi_bindgen_main`)
//! depois extrai a metadata desta crate e gera:
//!
//! - `bindings/swift/Sources/MCPixSDK/mcpix.swift`
//! - `bindings/kotlin/.../mcpix.kt`
//!
//! Por que separar de `mcpix-ffi`:
//! - O `mcpix-ffi` expõe C-ABI manual para .NET (P/Invoke) — símbolos curtos
//!   e estáveis, com `mcpix_*` como prefixo.
//! - O `mcpix-uniffi` exporta scaffolding do UniFFI (símbolos `uniffi_*`,
//!   estrutura de RustBuffer/ForeignBytes, etc.) — incompatível com a estética
//!   P/Invoke. Dois `cdylib`s, dois bindings — tabelas de símbolos limpas.

#![deny(rust_2018_idioms)]

use std::sync::Arc;

use mcpix_core::state::ValidationOutcome;
use mcpix_core::traits::SeedStore;
use mcpix_core::types::SeedId;
use mcpix_receiver_sdk::{
    memory_store::InMemorySeedStore, monotonic_counter::InMemoryCounter, system_random::OsRandom,
    ReceiverSdk,
};

// ─────────────────────────────────────────────────────────────────────────────
// Tipos expostos
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum McpixUniffiError {
    #[error("invalid seed id: {0}")]
    InvalidSeedId(String),
    #[error("unknown seed")]
    UnknownSeed,
    #[error("no retained receipt")]
    NoRetainedReceipt,
    #[error("counter overflow")]
    CounterOverflow,
    #[error("transport field invalid: {0}")]
    TransportField(String),
    #[error("storage failure: {0}")]
    Storage(String),
}

impl From<mcpix_core::error::McpixError> for McpixUniffiError {
    fn from(e: mcpix_core::error::McpixError) -> Self {
        use mcpix_core::error::McpixError as E;
        match e {
            E::SeedIdLength { .. } | E::SeedIdCharset => Self::InvalidSeedId(e.to_string()),
            E::UnknownSeed => Self::UnknownSeed,
            E::NoRetainedReceipt => Self::NoRetainedReceipt,
            E::CounterOverflow | E::CounterCollision { .. } | E::CounterRollback { .. } => {
                Self::CounterOverflow
            }
            E::TransportFieldLength(_)
            | E::TransportFieldCharset(_)
            | E::TransportFieldPrefix
            | E::UnsupportedProtocolVersion(_)
            | E::SeedLength { .. } => Self::TransportField(e.to_string()),
            E::Storage(_) | E::Transport(_) | E::Mismatch | E::ReplayRejected => {
                Self::Storage(e.to_string())
            }
        }
    }
}

/// Resultado de validação. Espelha o `ValidationOutcome` do núcleo num
/// formato que Swift/Kotlin tratam como enum nativo (sealed class / enum).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum McpixValidation {
    Valid,
    Mismatch,
    Replay,
}

impl From<ValidationOutcome> for McpixValidation {
    fn from(v: ValidationOutcome) -> Self {
        match v {
            ValidationOutcome::Valid => Self::Valid,
            ValidationOutcome::Mismatch => Self::Mismatch,
            ValidationOutcome::Replay => Self::Replay,
        }
    }
}

/// Saída de `generate_charge`: contém o campo público e o contador necessário
/// para validar mais tarde.
#[derive(Debug, Clone, uniffi::Record)]
pub struct McpixCharge {
    pub transport_field: String,
    pub counter: u64,
    pub amount_cents: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Objeto principal — `McpixReceiver`
// ─────────────────────────────────────────────────────────────────────────────

/// Fachada idiomática do SDK do recebedor. UniFFI traduz para:
/// - Swift: `class McpixReceiver { … }` (referência forte, ARC integrado)
/// - Kotlin: `class McpixReceiver : AutoCloseable { … }`
#[derive(uniffi::Object)]
pub struct McpixReceiver {
    inner: ReceiverSdk,
}

#[uniffi::export]
impl McpixReceiver {
    /// Construtor default: store in-memory, contador in-memory, RNG do OS.
    /// Versões futuras receberão delegates injetáveis (HttpTransport,
    /// SeedStore custom) seguindo a seção 3 da especificação.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: ReceiverSdk::new(
                Arc::new(InMemorySeedStore::new()),
                Arc::new(InMemoryCounter::new()),
                Arc::new(OsRandom),
            ),
        })
    }

    /// Restaura um recebedor a partir de um backup criptografado
    /// (formato `mcpix-backup`, Base58Check) + passphrase.
    ///
    /// Caso de uso central do sample Apple Wallet + App Clip: o App
    /// Clip resgata o blob selado no init e restaura a Seed
    /// localmente, ficando pronto para gerar cobranças **offline**.
    ///
    /// Apenas modo **sequencial** é suportado por este construtor —
    /// ele semeia o `InMemoryCounter` com o `T` restaurado para que a
    /// próxima cobrança use `T + 1` (preserva monotonia pós-troca de
    /// dispositivo). Backups em modo quantizado retornam erro: o
    /// counter quantizado deriva do relógio, não do estado salvo, e
    /// precisaria de um `TimestampQuantizedCounter` injetado — fora
    /// do escopo deste construtor simples.
    ///
    /// **Segurança**: a passphrase desbloqueia o Argon2id + AEAD do
    /// backup. Em produção venha de biometria/Keychain, não de input
    /// manual. Passphrase errada → erro de Storage (não distingue de
    /// blob corrompido — defesa de info-leak).
    #[uniffi::constructor]
    pub fn from_sealed_backup(
        backup: String,
        passphrase: String,
    ) -> Result<Arc<Self>, McpixUniffiError> {
        let restored = mcpix_backup::import(&backup, passphrase.as_bytes())
            .map_err(|e| McpixUniffiError::Storage(format!("backup restore: {e}")))?;

        if restored.counter_mode != mcpix_backup::CounterMode::Sequential {
            return Err(McpixUniffiError::Storage(
                "quantized-mode backup restore not supported by this constructor".into(),
            ));
        }

        let store = Arc::new(InMemorySeedStore::new());
        store.put_seed(&restored.seed_id, restored.seed)?;

        let counter = Arc::new(InMemoryCounter::new());
        counter.restore_last_issued(&restored.seed_id, restored.counter_t);

        Ok(Arc::new(Self {
            inner: ReceiverSdk::new(store, counter, Arc::new(OsRandom)),
        }))
    }

    /// Cadastra um recebedor com o `seed_id` informado.
    pub fn register(&self, seed_id: String) -> Result<(), McpixUniffiError> {
        let sid = SeedId::new(seed_id)?;
        self.inner.register(sid)?;
        Ok(())
    }

    /// Gera uma cobrança e devolve o campo de transporte público + contador.
    pub fn generate_charge(
        &self,
        seed_id: String,
        amount_cents: u64,
    ) -> Result<McpixCharge, McpixUniffiError> {
        let sid = SeedId::new(seed_id)?;
        let charge = self.inner.generate_charge(&sid, amount_cents)?;
        Ok(McpixCharge {
            transport_field: charge.transport_field,
            counter: charge.counter,
            amount_cents: charge.amount_cents,
        })
    }

    /// Valida um C₂ apresentado em tempo constante.
    pub fn validate_receipt(
        &self,
        seed_id: String,
        counter: u64,
        presented_c2: String,
    ) -> Result<McpixValidation, McpixUniffiError> {
        let sid = SeedId::new(seed_id)?;
        let outcome = self.inner.validate_receipt(&sid, counter, &presented_c2)?;
        Ok(outcome.into())
    }
}

// `uniffi::setup_scaffolding!` instala os pontos de entrada exigidos por
// `uniffi-bindgen`. Deve ser chamado exatamente uma vez por crate UniFFI.
uniffi::setup_scaffolding!();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_via_uniffi_layer() {
        let recv = McpixReceiver::new();
        recv.register("R1".into()).unwrap();
        let charge = recv.generate_charge("R1".into(), 9900).unwrap();
        assert!(charge.transport_field.starts_with("PIXOFFv1"));
        assert!(charge.counter > 0);
    }

    #[test]
    fn invalid_seed_id_is_typed_error() {
        let recv = McpixReceiver::new();
        let err = recv.register("R0".into()).unwrap_err();
        assert!(matches!(err, McpixUniffiError::InvalidSeedId(_)));
    }

    #[test]
    fn restore_from_sealed_backup_round_trips() {
        use mcpix_backup::{export_with_params_pub, CounterMode, ExportInput, KdfParams};
        use mcpix_core::types::Seed;

        // Produz um backup sequencial com T=5 (params rápidos p/ teste).
        let seed = Seed::from_bytes([0xAB; 32]);
        let sid = SeedId::new("RECVR1").unwrap();
        let input = ExportInput {
            seed: &seed,
            seed_id: &sid,
            counter_mode: CounterMode::Sequential,
            counter_t: 5,
        };
        let quick = KdfParams {
            m_cost_kib: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let blob = export_with_params_pub(&input, b"pwd", quick).unwrap();

        // Restaura via o construtor UniFFI.
        let recv = McpixReceiver::from_sealed_backup(blob, "pwd".into()).unwrap();

        // A próxima cobrança deve usar T = 6 (last_issued 5 + 1) —
        // monotonia preservada pós-restore.
        let charge = recv.generate_charge("RECVR1".into(), 100).unwrap();
        assert_eq!(charge.counter, 6, "counter must continue after restored T");
        assert!(charge.transport_field.starts_with("PIXOFFv1"));
    }

    #[test]
    fn restore_wrong_passphrase_is_typed_error() {
        use mcpix_backup::{export_with_params_pub, CounterMode, ExportInput, KdfParams};
        use mcpix_core::types::Seed;

        let seed = Seed::from_bytes([0x11; 32]);
        let sid = SeedId::new("R1").unwrap();
        let input = ExportInput {
            seed: &seed,
            seed_id: &sid,
            counter_mode: CounterMode::Sequential,
            counter_t: 1,
        };
        let quick = KdfParams {
            m_cost_kib: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let blob = export_with_params_pub(&input, b"right", quick).unwrap();

        // `unwrap_err()` exigiria `Arc<McpixReceiver>: Debug`; o objeto
        // UniFFI não deriva Debug, então casamos manualmente.
        match McpixReceiver::from_sealed_backup(blob, "wrong".into()) {
            Err(McpixUniffiError::Storage(_)) => {}
            Err(other) => panic!("expected Storage error, got {other:?}"),
            Ok(_) => panic!("wrong passphrase must not restore"),
        }
    }
}
