//! `SecureRandom` baseado no CSPRNG do sistema.
//!
//! Para os bindings nativos, esta impl é substituída pelo CSPRNG da plataforma
//! (SecRandomCopyBytes no iOS, SecureRandom no Android). Aqui usamos `getrandom`
//! via `rand_core::OsRng` para Linux/macOS/Windows.

use rand_core::{OsRng, RngCore};

use mcpix_core::error::McpixError;
use mcpix_core::traits::SecureRandom;

pub struct OsRandom;

impl SecureRandom for OsRandom {
    fn fill(&self, out: &mut [u8]) -> Result<(), McpixError> {
        // `try_fill_bytes` é o caminho não-panic do `RngCore`. Em hardware
        // saudável `OsRng` nunca falha, mas devolvemos erro estruturado para
        // jamais propagar panic via FFI.
        OsRng
            .try_fill_bytes(out)
            .map_err(|e| McpixError::Storage(format!("rng failure: {e}")))
    }
}
