//! Glue runtime para `mcpix_core::integrity` e `mcpix_core::signature`.
//!
//! O núcleo é zero-I/O por contrato — quem lê o `.so`/`.dylib`/`.dll` da
//! plataforma somos nós. Aqui buscamos:
//!
//! - o binário corrente (caminho via `MCPIX_SELF_PATH` ou `current_exe`)
//! - `SHA256SUMS` e `SHA256SUMS.sig` no diretório do binário
//!
//! e delegamos verificação ao núcleo.
//!
//! Política (S4):
//! - **Release build** (com `MCPIX_EXPECTED_SHA256` carimbado): exige
//!   `SHA256SUMS` + `SHA256SUMS.sig` válidos. Ausência ou assinatura
//!   inválida = `IntegrityCheck::Tampered`.
//! - **Dev build** (sem `MCPIX_EXPECTED_SHA256`): retorna `Skipped`.

use std::fs;
use std::path::{Path, PathBuf};

use mcpix_core::error::McpixError;
use mcpix_core::integrity::{sha256, verify_bytes, IntegrityCheck};
use mcpix_core::signature::{verify_combined, SignatureCheck, RELEASE_PUBKEY};

/// Hash esperado, embarcado pelo `build.rs` da crate-fachada. `None` em dev.
const EXPECTED_SHA256: Option<&str> = option_env!("MCPIX_EXPECTED_SHA256");

fn locate_self() -> Result<PathBuf, McpixError> {
    if let Ok(p) = std::env::var("MCPIX_SELF_PATH") {
        return Ok(PathBuf::from(p));
    }
    std::env::current_exe().map_err(|e| McpixError::Storage(e.to_string()))
}

/// Procura `SHA256SUMS` e `SHA256SUMS.sig` em ordem:
/// 1. Mesmo diretório do binário
/// 2. `../` (caso típico: `linux-x86_64/lib.so` ao lado de `SHA256SUMS` na raiz `dist/`)
fn locate_sums(binary_path: &Path) -> Option<(PathBuf, PathBuf)> {
    let parents = [binary_path.parent(), binary_path.parent().and_then(|p| p.parent())];
    for parent in parents.into_iter().flatten() {
        let sums = parent.join("SHA256SUMS");
        let sig = parent.join("SHA256SUMS.sig");
        if sums.exists() && sig.exists() {
            return Some((sums, sig));
        }
    }
    None
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Verifica integridade do binário carregado.
///
/// Comportamento detalhado:
/// - Dev build (sem `MCPIX_EXPECTED_SHA256`): `Skipped`.
/// - Release build com `SHA256SUMS.sig` presente:
///     - assinatura inválida → `Tampered`
///     - hash do binário não consta no manifest → `Tampered`
///     - hash bate → `Verified`
/// - Release build SEM manifest: cai no caminho legacy (S3) — verifica
///   apenas contra `MCPIX_EXPECTED_SHA256` carimbado. Útil para
///   integradores que ainda não distribuem o manifest assinado.
pub fn verify_self() -> Result<IntegrityCheck, McpixError> {
    let path = locate_self()?;
    let bytes = fs::read(&path)
        .map_err(|e| McpixError::Storage(format!("cannot read {}: {e}", path.display())))?;

    // Caminho legado (S3): sem manifest.
    let Some((sums_path, sig_path)) = locate_sums(&path) else {
        return verify_bytes(&bytes, EXPECTED_SHA256);
    };

    // Em dev build sem hash carimbado, mesmo com manifest presente devolvemos
    // Skipped — não temos âncora de confiança para validar contra.
    if EXPECTED_SHA256.is_none() {
        return Ok(IntegrityCheck::Skipped);
    }

    let sums = fs::read(&sums_path)
        .map_err(|e| McpixError::Storage(format!("cannot read {}: {e}", sums_path.display())))?;
    let sig = fs::read(&sig_path)
        .map_err(|e| McpixError::Storage(format!("cannot read {}: {e}", sig_path.display())))?;

    let actual = sha256(&bytes);
    let actual_hex = hex_of(&actual);
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| McpixError::Storage("non-utf8 binary filename".into()))?;

    let outcome = verify_combined(&sums, &sig, RELEASE_PUBKEY, filename, &actual_hex)?;
    Ok(match outcome {
        SignatureCheck::Verified => IntegrityCheck::Verified,
        SignatureCheck::Tampered { expected, .. } => {
            // Convertemos o expected (hex string ou marker) em bytes; se for um
            // marker textual ("absent..."), preenchemos com zeros — o caller
            // só usa o variant para abortar.
            let mut exp_bytes = [0u8; 32];
            if expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()) {
                for (i, chunk) in expected.as_bytes().chunks(2).enumerate() {
                    let s = std::str::from_utf8(chunk).unwrap();
                    exp_bytes[i] = u8::from_str_radix(s, 16).unwrap_or(0);
                }
            }
            IntegrityCheck::Tampered {
                expected: exp_bytes,
                actual,
            }
        }
        SignatureCheck::InvalidSignature | SignatureCheck::MalformedSums => {
            IntegrityCheck::Tampered {
                expected: [0; 32],
                actual,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn verify_self_skipped_in_dev_build() {
        // Em testes locais MCPIX_EXPECTED_SHA256 não está setado.
        let outcome = verify_self().unwrap();
        assert_eq!(outcome, IntegrityCheck::Skipped);
    }

    #[test]
    fn verify_self_with_override_reads_file() {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!("mcpix_integrity_test_{}", std::process::id()));
        {
            let mut f = std::fs::File::create(&tmp).unwrap();
            f.write_all(b"fake binary contents").unwrap();
        }
        std::env::set_var("MCPIX_SELF_PATH", &tmp);
        let outcome = verify_self().unwrap();
        std::env::remove_var("MCPIX_SELF_PATH");
        std::fs::remove_file(&tmp).ok();
        assert_eq!(outcome, IntegrityCheck::Skipped);
    }

    #[test]
    fn locate_sums_searches_two_levels() {
        let tmp = std::env::temp_dir().join(format!("mcpix_sums_{}", std::process::id()));
        let inner = tmp.join("linux-x86_64");
        std::fs::create_dir_all(&inner).unwrap();
        let bin = inner.join("libmcpix_uniffi.so");
        std::fs::write(&bin, b"x").unwrap();
        // SUMS no parent (caso típico do layout dist/)
        std::fs::write(tmp.join("SHA256SUMS"), b"").unwrap();
        std::fs::write(tmp.join("SHA256SUMS.sig"), b"").unwrap();

        let found = locate_sums(&bin);
        assert!(found.is_some());

        std::fs::remove_dir_all(&tmp).ok();
    }
}
