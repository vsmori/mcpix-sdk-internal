//! Verificação de assinatura Ed25519 e parsing de `SHA256SUMS`.
//!
//! **Modelo de confiança.** A chave pública canônica de release vive em
//! `crates/mcpix-core/trusted_keys/release.pub` (32 bytes raw Ed25519).
//! É embarcada no binário em compile-time via `include_bytes!` — para
//! substituí-la é necessário recompilar o crate, o que aumenta o custo
//! para um atacante que tenha acesso ao filesystem mas não ao código-fonte.
//!
//! **O que esta camada faz.** Recebe bytes opacos (conteúdo de `SHA256SUMS`,
//! assinatura de 64 bytes, hash hex do arquivo carregado) e responde
//! `Verified`/`Tampered`/`Invalid`. Não faz I/O — quem lê do disco é o
//! `mcpix-receiver-sdk::integrity_runtime`.
//!
//! **Limites.** Detecta substituição de qualquer artefato listado em
//! `SHA256SUMS`. Não detecta ataques in-memory (LD_PRELOAD, gdb), não
//! prova a identidade da chave embarcada (depende de revisão do .pub
//! no controle de versão).

use ed25519_dalek::{Signature, Verifier, VerifyingKey, PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH};

use crate::error::McpixError;

/// Tamanho fixo da chave pública Ed25519: 32 bytes raw.
pub const RELEASE_PUBKEY_LEN: usize = PUBLIC_KEY_LENGTH;

/// Tamanho fixo da assinatura Ed25519: 64 bytes raw.
pub const SIGNATURE_LEN: usize = SIGNATURE_LENGTH;

/// Chave pública canônica de release. Embarcada em compile-time para que
/// o atacante precise alterar o código-fonte para substituí-la — e qualquer
/// alteração quebra o SHA-256 self-check do próprio binário.
pub const RELEASE_PUBKEY: &[u8; RELEASE_PUBKEY_LEN] =
    include_bytes!("../trusted_keys/release.pub");

/// Resultado da verificação combinada (assinatura + hash).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignatureCheck {
    /// Assinatura é válida E o hash do arquivo bate com o listado em SUMS.
    Verified,
    /// Assinatura é válida mas o arquivo não consta em SUMS, ou consta com
    /// hash diferente — indica adulteração do binário.
    Tampered { expected: String, actual: String },
    /// Assinatura é inválida — SUMS foi adulterado ou chave errada.
    InvalidSignature,
    /// SUMS está mal-formado — não pôde ser parseado.
    MalformedSums,
}

/// Verifica assinatura Ed25519 sobre `sums_bytes` com `pubkey` (32 bytes).
pub fn verify_signature(
    sums_bytes: &[u8],
    signature_bytes: &[u8],
    pubkey: &[u8; RELEASE_PUBKEY_LEN],
) -> Result<bool, McpixError> {
    if signature_bytes.len() != SIGNATURE_LEN {
        return Err(McpixError::Storage(format!(
            "invalid signature length: expected {SIGNATURE_LEN}, got {}",
            signature_bytes.len()
        )));
    }
    let sig_array: [u8; SIGNATURE_LEN] = signature_bytes.try_into().unwrap();
    let signature = Signature::from_bytes(&sig_array);
    let vkey = VerifyingKey::from_bytes(pubkey)
        .map_err(|e| McpixError::Storage(format!("invalid pubkey: {e}")))?;
    Ok(vkey.verify(sums_bytes, &signature).is_ok())
}

/// Faz parse de uma linha `SHA256SUMS` no formato `<hex>  <path>`. Tolera
/// múltiplos espaços. Retorna `(hash_hex, path)`.
pub fn parse_sums_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut parts = line.splitn(2, char::is_whitespace);
    let hash = parts.next()?;
    let path = parts.next()?.trim_start();
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some((hash, path))
}

/// Verificação combinada: assinatura sobre `sums_bytes` é válida AND existe
/// linha em SUMS com `expected_filename` cujo hash bate com `actual_hash_hex`.
///
/// `expected_filename` é o basename do arquivo carregado (ex: `libmcpix_uniffi.so`).
/// A comparação é feita pelo final do path em cada linha de SUMS — assim o
/// mesmo SUMS funciona para layouts onde o path é absoluto, relativo, ou
/// inclui o nome da pasta `linux-x86_64/...`.
pub fn verify_combined(
    sums_bytes: &[u8],
    signature_bytes: &[u8],
    pubkey: &[u8; RELEASE_PUBKEY_LEN],
    expected_filename: &str,
    actual_hash_hex: &str,
) -> Result<SignatureCheck, McpixError> {
    let sig_ok = verify_signature(sums_bytes, signature_bytes, pubkey)?;
    if !sig_ok {
        return Ok(SignatureCheck::InvalidSignature);
    }
    let sums_text = match core::str::from_utf8(sums_bytes) {
        Ok(s) => s,
        Err(_) => return Ok(SignatureCheck::MalformedSums),
    };
    for line in sums_text.lines() {
        let Some((hash, path)) = parse_sums_line(line) else {
            continue;
        };
        if path.ends_with(expected_filename) {
            return if hash.eq_ignore_ascii_case(actual_hash_hex) {
                Ok(SignatureCheck::Verified)
            } else {
                Ok(SignatureCheck::Tampered {
                    expected: hash.to_string(),
                    actual: actual_hash_hex.to_string(),
                })
            };
        }
    }
    // Assinatura ok mas arquivo ausente do manifesto: tratamos como tampering
    // (o atacante poderia ter substituído o binário por algo não listado).
    Ok(SignatureCheck::Tampered {
        expected: format!("absent from SHA256SUMS for {expected_filename}"),
        actual: actual_hash_hex.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand_core::OsRng;

    fn fresh_keypair() -> (SigningKey, [u8; 32]) {
        let sk = SigningKey::generate(&mut OsRng);
        let pk = sk.verifying_key().to_bytes();
        (sk, pk)
    }

    #[test]
    fn good_signature_verifies() {
        let (sk, pk) = fresh_keypair();
        let msg = b"the SHA256SUMS file content";
        let sig = sk.sign(msg).to_bytes();
        assert!(verify_signature(msg, &sig, &pk).unwrap());
    }

    #[test]
    fn altered_message_fails() {
        let (sk, pk) = fresh_keypair();
        let sig = sk.sign(b"original").to_bytes();
        assert!(!verify_signature(b"tampered", &sig, &pk).unwrap());
    }

    #[test]
    fn wrong_key_fails() {
        let (sk, _) = fresh_keypair();
        let (_, pk2) = fresh_keypair();
        let sig = sk.sign(b"hi").to_bytes();
        assert!(!verify_signature(b"hi", &sig, &pk2).unwrap());
    }

    #[test]
    fn invalid_signature_length_errors() {
        let (_, pk) = fresh_keypair();
        let err = verify_signature(b"x", &[0u8; 10], &pk).unwrap_err();
        assert!(matches!(err, McpixError::Storage(_)));
    }

    #[test]
    fn parse_sums_line_handles_common_formats() {
        assert_eq!(
            parse_sums_line(
                "abc123abc123abc123abc123abc123abc123abc123abc123abc123abc123abcd  linux-x86_64/libmcpix_uniffi.so"
            ),
            Some((
                "abc123abc123abc123abc123abc123abc123abc123abc123abc123abc123abcd",
                "linux-x86_64/libmcpix_uniffi.so"
            ))
        );
        assert_eq!(parse_sums_line(""), None);
        assert_eq!(parse_sums_line("only-one-token"), None);
        assert_eq!(parse_sums_line("toosshort  file"), None);
    }

    #[test]
    fn combined_verifies_when_listed_and_signed() {
        let (sk, pk) = fresh_keypair();
        let hash = "aa".repeat(32);
        let sums = format!("{hash}  linux-x86_64/libmcpix_uniffi.so\n");
        let sig = sk.sign(sums.as_bytes()).to_bytes();
        let outcome =
            verify_combined(sums.as_bytes(), &sig, &pk, "libmcpix_uniffi.so", &hash).unwrap();
        assert_eq!(outcome, SignatureCheck::Verified);
    }

    #[test]
    fn combined_detects_hash_mismatch() {
        let (sk, pk) = fresh_keypair();
        let listed_hash = "aa".repeat(32);
        let actual_hash = "bb".repeat(32);
        let sums = format!("{listed_hash}  libmcpix_uniffi.so\n");
        let sig = sk.sign(sums.as_bytes()).to_bytes();
        match verify_combined(sums.as_bytes(), &sig, &pk, "libmcpix_uniffi.so", &actual_hash)
            .unwrap()
        {
            SignatureCheck::Tampered { .. } => {}
            other => panic!("expected Tampered, got {other:?}"),
        }
    }

    #[test]
    fn combined_detects_bad_signature() {
        let (sk, _) = fresh_keypair();
        let (_, pk2) = fresh_keypair();
        let hash = "aa".repeat(32);
        let sums = format!("{hash}  libmcpix_uniffi.so\n");
        let sig = sk.sign(sums.as_bytes()).to_bytes();
        let outcome =
            verify_combined(sums.as_bytes(), &sig, &pk2, "libmcpix_uniffi.so", &hash).unwrap();
        assert_eq!(outcome, SignatureCheck::InvalidSignature);
    }

    #[test]
    fn combined_detects_file_not_listed() {
        let (sk, pk) = fresh_keypair();
        let hash = "aa".repeat(32);
        let sums = format!("{hash}  some-other-file.so\n");
        let sig = sk.sign(sums.as_bytes()).to_bytes();
        match verify_combined(sums.as_bytes(), &sig, &pk, "libmcpix_uniffi.so", &hash).unwrap() {
            SignatureCheck::Tampered { expected, .. } => {
                assert!(expected.contains("absent"));
            }
            other => panic!("expected Tampered, got {other:?}"),
        }
    }

    #[test]
    fn release_pubkey_is_well_formed() {
        // Garante que o pub key commitada é uma chave Ed25519 válida (não
        // 32 bytes aleatórios). Falha cedo no build em vez de runtime.
        assert_eq!(RELEASE_PUBKEY.len(), 32);
        let _ = VerifyingKey::from_bytes(RELEASE_PUBKEY)
            .expect("trusted_keys/release.pub is not a valid ed25519 key");
    }
}
