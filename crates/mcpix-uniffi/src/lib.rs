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
use mcpix_core::types::SeedId;
use mcpix_receiver_sdk::{
    memory_store::InMemorySeedStore, monotonic_counter::InMemoryCounter,
    system_random::OsRandom, ReceiverSdk,
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
            E::CounterOverflow
            | E::CounterCollision { .. }
            | E::CounterRollback { .. } => Self::CounterOverflow,
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
}
