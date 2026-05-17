//! Verifica integridade fim-a-fim sobre o artefato compilado em `dist/`.
//!
//! Diferente dos testes unitários (que rodam em dev-mode sem
//! `MCPIX_EXPECTED_SHA256` carimbado), este integration test computa o hash
//! do `.so` real produzido por `cargo xtask build-linux` e exercita o
//! caminho `verify_bytes` que o `verify_self` chama internamente.
//!
//! Marcado `#[ignore]`: depende de `dist/` existir. Roda no pipeline após
//! `build-linux` ou localmente via `cargo test -- --ignored`.

use std::path::PathBuf;

use mcpix_core::integrity::{verify_bytes, IntegrityCheck};

fn dist_lib() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("../..")
        .canonicalize()
        .unwrap()
        .join("dist/linux-x86_64/libmcpix_uniffi.so")
}

#[test]
#[ignore = "requires `cargo xtask build-linux` to have produced dist/"]
fn verify_real_artifact_matches_its_own_hash() {
    let path = dist_lib();
    let bytes = std::fs::read(&path).expect("dist artifact missing — run `cargo xtask build-linux`");

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hex: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();

    let outcome = verify_bytes(&bytes, Some(&hex)).unwrap();
    assert_eq!(outcome, IntegrityCheck::Verified);
}

#[test]
#[ignore = "requires `cargo xtask build-linux`"]
fn verify_detects_tampering_in_real_artifact() {
    let path = dist_lib();
    let mut bytes = std::fs::read(&path).unwrap();

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hex: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();

    // Flip 1 byte — qualquer alteração tem que invalidar.
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;

    match verify_bytes(&bytes, Some(&hex)).unwrap() {
        IntegrityCheck::Tampered { .. } => {}
        other => panic!("expected Tampered, got {other:?}"),
    }
}
