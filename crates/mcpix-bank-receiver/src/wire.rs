//! Tipos compartilhados entre `http_server` e `http_client`. Forma a "wire
//! contract" do endpoint REST do banco recebedor.
//!
//! Formato deliberadamente simples: nada que o protocolo central precise
//! interpretar — só payloads de armazenamento e lookup. Auth real é externa
//! (mTLS no termination ou Bearer token no header — adicionada em sessão
//! futura, junto com PKI).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use mcpix_core::error::McpixError;
use mcpix_core::types::{Seed, SEED_LEN};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedPayload {
    /// Material da semente em base64 padrão (4*ceil(32/3)=44 chars). Em
    /// produção transitaria sob mTLS e idealmente envelopado por key-wrap —
    /// aqui claro, suficiente para a demo.
    pub material_b64: String,
}

impl SeedPayload {
    pub fn from_seed(seed: &Seed) -> Self {
        Self {
            material_b64: B64.encode(seed.as_bytes()),
        }
    }

    pub fn into_seed(self) -> Result<Seed, McpixError> {
        let bytes = B64
            .decode(self.material_b64.as_bytes())
            .map_err(|e| McpixError::Transport(format!("invalid base64: {e}")))?;
        if bytes.len() != SEED_LEN {
            return Err(McpixError::SeedLength {
                expected: SEED_LEN,
                got: bytes.len(),
            });
        }
        Seed::try_from_slice(&bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: String,
}
