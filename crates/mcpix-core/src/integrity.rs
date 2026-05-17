//! Verificação de integridade do binário (Bloco 4.2 da especificação).
//!
//! **Por que existe.** O SDK opera no mercado financeiro. Um atacante que
//! consegue substituir o `.so`/`.dll`/`.dylib` carregado pelo processo pode
//! mudar a derivação de C₂ silenciosamente — comprovantes válidos passam a
//! ser falsificáveis sem que o recebedor perceba. Este módulo carimba um
//! hash SHA-256 do conteúdo binário ESPERADO da biblioteca no momento da
//! geração do artefato (via `cargo xtask`/CI) e o compara com o hash
//! computado sobre o arquivo carregado na primeira inicialização.
//!
//! Limites do que isto cobre:
//! - Detecta substituição inteira do arquivo. Sim.
//! - Detecta patches in-place de bytes da `.text`. Sim.
//! - Detecta ataques runtime-only (LD_PRELOAD, gdb), DLL hijacking de deps,
//!   ou comprometimento do processo hospedeiro. **Não.** Para isso seriam
//!   necessários remote attestation/TEE — fora do escopo da S3.
//!
//! O hash esperado é injetado em tempo de build via `MCPIX_EXPECTED_SHA256`
//! (variável de ambiente lida pelo `build.rs`). Quando ausente (build local
//! de desenvolvimento), a verificação é "soft" — retorna `Skipped` em vez
//! de `Tampered`. O CI sempre roda com a variável presente.

use sha2::{Digest, Sha256};

use crate::error::McpixError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntegrityCheck {
    /// Hash do arquivo coincide com o esperado.
    Verified,
    /// Hash não coincide — possível adulteração. Caller deve abortar.
    Tampered { expected: [u8; 32], actual: [u8; 32] },
    /// Hash esperado não foi embarcado neste build (dev mode).
    Skipped,
}

/// Computa SHA-256 sobre os bytes fornecidos. Usar apenas em caminhos de
/// inicialização — a função aloca para conveniência da API.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&out);
    buf
}

/// Verifica integridade comparando o hash do binário carregado com um hash
/// esperado fornecido como hex (64 chars). Use `None` para indicar build
/// de desenvolvimento.
///
/// Esta função é puramente computacional. As fachadas (`mcpix-receiver-sdk`,
/// `mcpix-uniffi`) leem o caminho do próprio `.so`/`.dylib` via APIs da
/// plataforma e passam os bytes para cá — manter a leitura de arquivo fora
/// preserva a propriedade de zero-I/O do núcleo.
pub fn verify_bytes(
    actual_bytes: &[u8],
    expected_hex: Option<&str>,
) -> Result<IntegrityCheck, McpixError> {
    let Some(hex) = expected_hex else {
        return Ok(IntegrityCheck::Skipped);
    };
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(McpixError::Storage(format!(
            "invalid expected hash length/charset: {} chars",
            hex.len()
        )));
    }
    let mut expected = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = core::str::from_utf8(chunk).map_err(|e| McpixError::Storage(e.to_string()))?;
        expected[i] =
            u8::from_str_radix(s, 16).map_err(|e| McpixError::Storage(e.to_string()))?;
    }
    let actual = sha256(actual_bytes);
    if expected == actual {
        Ok(IntegrityCheck::Verified)
    } else {
        Ok(IntegrityCheck::Tampered { expected, actual })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        // "abc" → ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let h = sha256(b"abc");
        let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn verify_skipped_when_no_expected() {
        assert_eq!(
            verify_bytes(b"anything", None).unwrap(),
            IntegrityCheck::Skipped
        );
    }

    #[test]
    fn verify_matches_when_hash_equal() {
        let payload = b"the SDK bytes";
        let hex: String = sha256(payload).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            verify_bytes(payload, Some(&hex)).unwrap(),
            IntegrityCheck::Verified
        );
    }

    #[test]
    fn verify_detects_tampering() {
        let original = b"the SDK bytes";
        let tampered = b"the SDK byteS"; // 1-byte flip
        let hex: String = sha256(original).iter().map(|b| format!("{b:02x}")).collect();
        match verify_bytes(tampered, Some(&hex)).unwrap() {
            IntegrityCheck::Tampered { .. } => {}
            other => panic!("expected Tampered, got {other:?}"),
        }
    }

    #[test]
    fn verify_rejects_malformed_hex() {
        let err = verify_bytes(b"x", Some("not-hex")).unwrap_err();
        assert!(matches!(err, McpixError::Storage(_)));
    }
}
