//! Cross-validation entre `mcpix-backup` (host) e `mcpix-embed::restore`
//! (no_std mas testado no host via std).
//!
//! Garantia: backup gerado pelo host é decifrado bit-exato pelo embed.
//! Se este teste quebrar, os dois parsers do wire format divergiram —
//! falha silenciosa em produção (telefone faz backup, MCU não restaura).

#![cfg(feature = "restore")]

use mcpix_backup::{export_with_params_pub, ExportInput, KdfParams};
use mcpix_core::types::{Seed as CoreSeed, SeedId as CoreSeedId};
use mcpix_embed::restore::{import as embed_import, CounterMode as EmbedCounterMode};

fn quick_params() -> KdfParams {
    KdfParams {
        m_cost_kib: 8,
        t_cost: 1,
        p_cost: 1,
    }
}

#[test]
fn host_export_decrypts_in_embed_sequential_mode() {
    let seed = CoreSeed::from_bytes([0x55; 32]);
    let sid = CoreSeedId::new("RECVR1").unwrap();
    let input = ExportInput {
        seed: &seed,
        seed_id: &sid,
        counter_mode: mcpix_backup::CounterMode::Sequential,
        counter_t: 123,
    };
    let text = export_with_params_pub(&input, b"pwd", quick_params()).unwrap();
    let restored = embed_import(&text, b"pwd").expect("embed import");

    assert_eq!(restored.seed.as_bytes(), &[0x55u8; 32]);
    assert_eq!(restored.seed_id.as_str(), "RECVR1");
    assert_eq!(restored.counter_mode, EmbedCounterMode::Sequential);
    assert_eq!(restored.counter_t, 123);
}

#[test]
fn host_export_decrypts_in_embed_quantized_mode() {
    let seed = CoreSeed::from_bytes([0xAA; 32]);
    let sid = CoreSeedId::new("Bank42").unwrap();
    let input = ExportInput {
        seed: &seed,
        seed_id: &sid,
        counter_mode: mcpix_backup::CounterMode::Quantized,
        counter_t: 0,
    };
    let text = export_with_params_pub(&input, b"another-pwd", quick_params()).unwrap();
    let restored = embed_import(&text, b"another-pwd").expect("embed import");

    assert_eq!(restored.counter_mode, EmbedCounterMode::Quantized);
    assert_eq!(restored.counter_t, 0);
}

#[test]
fn embed_rejects_wrong_passphrase_from_host_export() {
    let seed = CoreSeed::from_bytes([0x11; 32]);
    let sid = CoreSeedId::new("R1").unwrap();
    let input = ExportInput {
        seed: &seed,
        seed_id: &sid,
        counter_mode: mcpix_backup::CounterMode::Sequential,
        counter_t: 1,
    };
    let text = export_with_params_pub(&input, b"right", quick_params()).unwrap();
    let err = embed_import(&text, b"wrong").unwrap_err();
    assert_eq!(err, mcpix_embed::restore::RestoreError::DecryptFailed);
}
