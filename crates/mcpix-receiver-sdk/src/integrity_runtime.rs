//! Glue runtime para `mcpix_core::integrity`.
//!
//! O núcleo é zero-I/O por contrato — quem lê o `.so`/`.dylib`/`.dll` da
//! plataforma somos nós. Aqui buscamos o caminho do binário corrente via
//! `std::env::current_exe`/`dladdr` e delegamos o hash à crate-núcleo.
//!
//! Hash esperado vem da variável `MCPIX_EXPECTED_SHA256` capturada em build
//! time. Quando ausente (build de dev), retornamos `Skipped`.

use std::fs;
use std::path::PathBuf;

use mcpix_core::error::McpixError;
use mcpix_core::integrity::{verify_bytes, IntegrityCheck};

/// Hash esperado, embarcado pelo `build.rs` da crate-fachada. `None` em dev.
const EXPECTED_SHA256: Option<&str> = option_env!("MCPIX_EXPECTED_SHA256");

/// Caminho candidato a verificar. Em ordem:
/// 1. `MCPIX_SELF_PATH` (override explícito — testes, smoke checks)
/// 2. Caminho da biblioteca dinâmica que contém este código (via `dladdr`)
/// 3. `std::env::current_exe()` (último recurso — executável hospedeiro)
fn locate_self() -> Result<PathBuf, McpixError> {
    if let Ok(p) = std::env::var("MCPIX_SELF_PATH") {
        return Ok(PathBuf::from(p));
    }
    // `dladdr` exigiria libc + unsafe. Mantemos esta crate `forbid(unsafe_code)`,
    // então delegamos a localização à plataforma hospedeira em produção: o
    // binding (Swift/Kotlin/.NET) passa o caminho via `MCPIX_SELF_PATH` antes
    // de invocar `verify_self()`. Aqui, na ausência, caímos no executável.
    std::env::current_exe().map_err(|e| McpixError::Storage(e.to_string()))
}

/// Verifica integridade do binário carregado. Caller deve chamar uma vez na
/// inicialização e abortar em `Tampered`.
pub fn verify_self() -> Result<IntegrityCheck, McpixError> {
    let path = locate_self()?;
    let bytes = fs::read(&path).map_err(|e| {
        McpixError::Storage(format!("cannot read binary at {}: {e}", path.display()))
    })?;
    verify_bytes(&bytes, EXPECTED_SHA256)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn verify_self_skipped_in_dev_build() {
        // Em testes locais MCPIX_EXPECTED_SHA256 não está setado, então o
        // resultado deve ser `Skipped` — confirmando que dev builds não
        // falham por ausência de hash.
        let outcome = verify_self().unwrap();
        assert_eq!(outcome, IntegrityCheck::Skipped);
    }

    #[test]
    fn verify_self_with_override_reads_file() {
        // Cria arquivo temporário, aponta MCPIX_SELF_PATH para ele e força
        // verificação. Como não há expected hash, ainda dá Skipped — mas
        // exercita o caminho de leitura.
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("mcpix_integrity_test_{}", std::process::id()));
        {
            let mut f = std::fs::File::create(&tmp).unwrap();
            f.write_all(b"fake binary contents").unwrap();
        }
        // SAFETY: ajuste de env var é seguro em testes single-threaded por
        // path. Aqui o teste é isolado por PID no nome do arquivo.
        std::env::set_var("MCPIX_SELF_PATH", &tmp);
        let outcome = verify_self().unwrap();
        std::env::remove_var("MCPIX_SELF_PATH");
        std::fs::remove_file(&tmp).ok();
        assert_eq!(outcome, IntegrityCheck::Skipped);
    }
}
