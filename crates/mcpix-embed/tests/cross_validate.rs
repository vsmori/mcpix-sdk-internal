//! Cross-validation entre `mcpix-embed` (no_std) e `mcpix-core` (std).
//!
//! Garantia: para todo `(seed, counter)`, a derivação no microcontrolador
//! produz exatamente os mesmos bytes que o servidor (banco do pagador) vai
//! produzir. Se este teste quebrar, o protocolo está dessincronizado entre
//! as duas implementações — falha catastrófica não-óbvia em runtime.
//!
//! Rodado no host (precisa de `std`). Em firmware o `mcpix-core` não está
//! presente; aqui é validação de equivalência.

#[test]
fn derive_pair_matches_core_for_sampled_inputs() {
    let cases: &[(u64, [u8; 32])] = &[
        (0, [0x00; 32]),
        (1, [0xAB; 32]),
        (42, [0x42; 32]),
        (u64::MAX, [0xFF; 32]),
        (1_700_000_000 / 30, [0x77; 32]),
        (1_700_000_000 / 30 + 1, [0x77; 32]),
    ];

    for (counter, seed_bytes) in cases {
        let core_seed = mcpix_core::types::Seed::from_bytes(*seed_bytes);
        let (core_c1, core_c2) = mcpix_core::crypto::derive_pair(&core_seed, *counter);

        let embed_seed = mcpix_embed::types::Seed::from_bytes(*seed_bytes);
        let (embed_c1, embed_c2) = mcpix_embed::crypto::derive_pair(&embed_seed, *counter);

        assert_eq!(
            embed_c1.as_str(),
            core_c1.as_str(),
            "C1 mismatch for counter={counter}, seed[0]={}",
            seed_bytes[0]
        );
        assert_eq!(
            embed_c2.as_str(),
            core_c2.as_str(),
            "C2 mismatch for counter={counter}, seed[0]={}",
            seed_bytes[0]
        );
    }
}

#[test]
fn encode_field_matches_core() {
    use mcpix_embed::transport_field::{encode_into, TRANSPORT_FIELD_LEN};
    let seed_bytes = [0x33u8; 32];
    let counter = 5u64;

    let core_seed = mcpix_core::types::Seed::from_bytes(seed_bytes);
    let core_sid = mcpix_core::types::SeedId::new("R1").unwrap();
    let (core_c1, _) = mcpix_core::crypto::derive_pair(&core_seed, counter);
    let core_field = mcpix_core::transport_field::encode(&core_sid, &core_c1);

    let embed_seed = mcpix_embed::types::Seed::from_bytes(seed_bytes);
    let embed_sid = mcpix_embed::types::SeedId::new("R1").unwrap();
    let (embed_c1, _) = mcpix_embed::crypto::derive_pair(&embed_seed, counter);
    let mut buf = [0u8; TRANSPORT_FIELD_LEN];
    let embed_field = encode_into(&embed_sid, &embed_c1, &mut buf);

    assert_eq!(embed_field, core_field.as_str());
}

#[test]
fn varied_seed_id_lengths_match() {
    use mcpix_embed::transport_field::{encode_into, TRANSPORT_FIELD_LEN};
    for sid_str in ["R1", "Bank", "ACME123", "abcdefghijklmnop"] {
        let core_sid = mcpix_core::types::SeedId::new(sid_str).unwrap();
        let embed_sid = mcpix_embed::types::SeedId::new(sid_str).unwrap();

        let seed = [0x9Fu8; 32];
        let counter = 7u64;
        let (core_c1, _) = mcpix_core::crypto::derive_pair(
            &mcpix_core::types::Seed::from_bytes(seed),
            counter,
        );
        let (embed_c1, _) = mcpix_embed::crypto::derive_pair(
            &mcpix_embed::types::Seed::from_bytes(seed),
            counter,
        );
        assert_eq!(embed_c1.as_str(), core_c1.as_str());

        let core_field = mcpix_core::transport_field::encode(&core_sid, &core_c1);
        let mut buf = [0u8; TRANSPORT_FIELD_LEN];
        let embed_field = encode_into(&embed_sid, &embed_c1, &mut buf);
        assert_eq!(embed_field, core_field.as_str(), "drift for sid={sid_str}");
    }
}
