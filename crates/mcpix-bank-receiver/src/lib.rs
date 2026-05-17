//! Módulo backend do banco do recebedor.
//!
//! Custodia as sementes `S` dos recebedores que cadastrou. Em produção esta
//! camada vive atrás de uma HSM (chamadas a `lookup_seed` exigiriam autenticação
//! mTLS e auditoria). Aqui mantemos a **mesma interface** para que a substituição
//! por HSM real seja transparente para o caller.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]

#[cfg(feature = "http-server")]
pub mod http_server;

#[cfg(feature = "http-client")]
pub mod http_client;

#[cfg(any(feature = "http-server", feature = "http-client"))]
mod wire;

use std::collections::HashMap;

use parking_lot::RwLock;

use mcpix_core::error::McpixError;
use mcpix_core::types::{Seed, SeedId};

/// Contrato do banco recebedor — equivalente em escopo a `SeedStore`, porém
/// distinto semanticamente (institucional, não local). Mantemos traits separadas
/// para não amarrar caminhos de chamada.
pub trait BankReceiver: Send + Sync {
    fn register_seed(&self, receiver_id: &SeedId, seed: Seed) -> Result<(), McpixError>;
    fn lookup_seed(&self, receiver_id: &SeedId, requester: &Requester)
        -> Result<Seed, McpixError>;
}

/// Identidade do requerente em uma consulta inter-institucional. Hoje só
/// carrega um identificador opaco; preparada para incluir `client_cert_fingerprint`,
/// `mTLS subject`, etc.
#[derive(Clone, Debug)]
pub struct Requester {
    pub institution_id: String,
}

#[derive(Default)]
pub struct InMemoryBankReceiver {
    inner: RwLock<HashMap<SeedId, Seed>>,
}

impl InMemoryBankReceiver {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BankReceiver for InMemoryBankReceiver {
    fn register_seed(&self, receiver_id: &SeedId, seed: Seed) -> Result<(), McpixError> {
        self.inner.write().insert(receiver_id.clone(), seed);
        Ok(())
    }

    fn lookup_seed(
        &self,
        receiver_id: &SeedId,
        _requester: &Requester,
    ) -> Result<Seed, McpixError> {
        // Ponto de extensão: aqui entraria a verificação de autorização do
        // `_requester`. Na demo, qualquer instituição pode consultar.
        self.inner
            .read()
            .get(receiver_id)
            .cloned()
            .ok_or(McpixError::UnknownSeed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_lookup() {
        let bank = InMemoryBankReceiver::new();
        let sid = SeedId::new("R1").unwrap();
        bank.register_seed(&sid, Seed::from_bytes([7; 32])).unwrap();
        let s = bank
            .lookup_seed(&sid, &Requester { institution_id: "PAYER_BANK".into() })
            .unwrap();
        assert_eq!(s.as_bytes(), &[7u8; 32]);
    }

    #[test]
    fn lookup_unknown_fails() {
        let bank = InMemoryBankReceiver::new();
        let err = bank
            .lookup_seed(
                &SeedId::new("ghost").unwrap(),
                &Requester { institution_id: "X".into() },
            )
            .unwrap_err();
        assert_eq!(err, McpixError::UnknownSeed);
    }
}
