//! Verifica integridade fim-a-fim sobre o artefato compilado em `dist/`.
//!
//! Diferente dos testes unitários (que rodam em dev-mode sem
//! `MCPIX_EXPECTED_SHA256` carimbado), este integration test computa o hash
//! do `.so` real produzido por `cargo xtask build-linux` e exercita o
//! caminho `verify_bytes` / `verify_combined` do núcleo.
//!
//! Marcado `#[ignore]`: depende de `dist/` existir. Roda no pipeline após
//! `build-linux` + `hash-artifacts` + `sign-artifacts`, ou localmente via
//! `cargo test -- --ignored`.

use std::path::PathBuf;

use mcpix_core::integrity::{sha256, verify_bytes, IntegrityCheck};
use mcpix_core::signature::{verify_combined, SignatureCheck, RELEASE_PUBKEY};

fn dist_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../..").canonicalize().unwrap().join("dist")
}

fn read_dist(rel: &str) -> Vec<u8> {
    std::fs::read(dist_root().join(rel))
        .unwrap_or_else(|e| panic!("missing dist/{rel}: {e} — run xtask build-linux + hash + sign"))
}

#[test]
#[ignore = "requires `cargo xtask build-linux` to have produced dist/"]
fn verify_real_artifact_matches_its_own_hash() {
    let bytes = read_dist("linux-x86_64/libmcpix_uniffi.so");
    let hex: String = sha256(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    let outcome = verify_bytes(&bytes, Some(&hex)).unwrap();
    assert_eq!(outcome, IntegrityCheck::Verified);
}

#[test]
#[ignore = "requires `cargo xtask build-linux`"]
fn verify_detects_tampering_in_real_artifact() {
    let mut bytes = read_dist("linux-x86_64/libmcpix_uniffi.so");
    let hex: String = sha256(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    match verify_bytes(&bytes, Some(&hex)).unwrap() {
        IntegrityCheck::Tampered { .. } => {}
        other => panic!("expected Tampered, got {other:?}"),
    }
}

#[test]
#[ignore = "requires hash-artifacts + sign-artifacts to have run"]
fn signed_manifest_verifies_against_release_pubkey() {
    let bytes = read_dist("linux-x86_64/libmcpix_uniffi.so");
    let sums = read_dist("SHA256SUMS");
    let sig = read_dist("SHA256SUMS.sig");
    let hex: String = sha256(&bytes).iter().map(|b| format!("{b:02x}")).collect();

    let outcome =
        verify_combined(&sums, &sig, RELEASE_PUBKEY, "libmcpix_uniffi.so", &hex).unwrap();
    assert_eq!(outcome, SignatureCheck::Verified);
}

#[test]
#[ignore = "requires hash-artifacts + sign-artifacts"]
fn signed_manifest_detects_swapped_binary() {
    let sums = read_dist("SHA256SUMS");
    let sig = read_dist("SHA256SUMS.sig");
    // Hash inventado que não bate com nenhum arquivo listado
    let bogus = "ff".repeat(32);
    match verify_combined(&sums, &sig, RELEASE_PUBKEY, "libmcpix_uniffi.so", &bogus).unwrap() {
        SignatureCheck::Tampered { .. } => {}
        other => panic!("expected Tampered, got {other:?}"),
    }
}

#[test]
#[ignore = "requires hash-artifacts + sign-artifacts"]
fn signed_manifest_detects_tampered_sums() {
    let mut sums = read_dist("SHA256SUMS");
    let sig = read_dist("SHA256SUMS.sig");
    // Adultera 1 byte do SUMS — assinatura tem que falhar
    sums[0] ^= 0x01;
    match verify_combined(&sums, &sig, RELEASE_PUBKEY, "libmcpix_uniffi.so", &"00".repeat(32))
        .unwrap()
    {
        SignatureCheck::InvalidSignature => {}
        other => panic!("expected InvalidSignature, got {other:?}"),
    }
}
