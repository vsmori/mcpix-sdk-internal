//! Property-based tests do núcleo criptográfico.
//!
//! Modelo: cada propriedade é uma invariante que deve valer para **todo**
//! input bem-formado dentro do domínio do contrato. `proptest` gera 1000+
//! casos por propriedade (configurável via `PROPTEST_CASES`).
//!
//! Foco intencional: estes testes existem para defender as **reivindicações
//! técnicas do pedido PCT** — determinismo, encadeamento, tempo constante,
//! parsing robusto. Quebra aqui = revisão da reivindicação.

use mcpix_core::crypto::{derive_c2_from_c1, derive_pair, verify_c2};
use mcpix_core::signature::{parse_sums_line, verify_combined, SignatureCheck, RELEASE_PUBKEY};
use mcpix_core::transport_field::{encode, is_protocol_field, parse, PROTOCOL_PREFIX};
use mcpix_core::types::{Seed, SeedId, C1_TRANSPORT_LEN, C2_TRANSPORT_LEN};
use proptest::prelude::*;

// ─────────────────────────────────────────────────────────────────────────────
// Estratégias (geradores) reutilizáveis
// ─────────────────────────────────────────────────────────────────────────────

/// Seed arbitrária (32 bytes quaisquer).
fn seed_strategy() -> impl Strategy<Value = Seed> {
    prop::array::uniform32(any::<u8>()).prop_map(Seed::from_bytes)
}

/// SeedId válido: 1..=16 chars do alfabeto `[a-zA-Z1-9]` (sem `'0'`, reservado como pad).
fn seed_id_strategy() -> impl Strategy<Value = SeedId> {
    // Regex que casa string de tamanho 1..=16 com cada char do alfabeto.
    prop::string::string_regex("[a-zA-Z1-9]{1,16}")
        .unwrap()
        .prop_map(|s| SeedId::new(s).unwrap())
}

/// Counter qualquer, mas limitado a u64 — não usamos u128 no protocolo.
fn counter_strategy() -> impl Strategy<Value = u64> {
    any::<u64>()
}

// ─────────────────────────────────────────────────────────────────────────────
// Propriedades — núcleo criptográfico
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    /// Determinismo: chamadas repetidas com (S, T) iguais retornam (C₁, C₂)
    /// idênticos. Este é o requisito que viabiliza a substituição institucional.
    #[test]
    fn derive_pair_is_deterministic(s in seed_strategy(), t in counter_strategy()) {
        let (c1a, c2a) = derive_pair(&s, t);
        let (c1b, c2b) = derive_pair(&s, t);
        prop_assert_eq!(c1a.as_str(), c1b.as_str());
        prop_assert_eq!(c2a.as_str(), c2b.as_str());
    }

    /// Encadeamento: C₂ retornado por derive_pair é igual ao C₂ recomposto
    /// via derive_c2_from_c1 a partir do C₁ correspondente. Garante que o
    /// banco do pagador chega no mesmo C₂ que o recebedor offline.
    #[test]
    fn c2_recoverable_from_c1(s in seed_strategy(), t in counter_strategy()) {
        let (c1, c2_pair) = derive_pair(&s, t);
        let c2_recon = derive_c2_from_c1(&s, t, &c1);
        prop_assert_eq!(c2_pair.as_str(), c2_recon.as_str());
    }

    /// Distinção por counter: T diferentes geram C₁ diferentes (com prob.
    /// astronomicamente alta — colisão de HMAC truncado).
    #[test]
    fn different_counter_yields_different_c1(
        s in seed_strategy(),
        t1 in counter_strategy(),
        t2 in counter_strategy(),
    ) {
        prop_assume!(t1 != t2);
        let (c1a, _) = derive_pair(&s, t1);
        let (c1b, _) = derive_pair(&s, t2);
        prop_assert_ne!(c1a.as_str(), c1b.as_str());
    }

    /// Distinção por seed: sementes diferentes geram pares diferentes.
    #[test]
    fn different_seed_yields_different_c1(
        s1 in seed_strategy(),
        s2 in seed_strategy(),
        t in counter_strategy(),
    ) {
        prop_assume!(s1.as_bytes() != s2.as_bytes());
        let (c1a, _) = derive_pair(&s1, t);
        let (c1b, _) = derive_pair(&s2, t);
        prop_assert_ne!(c1a.as_str(), c1b.as_str());
    }

    /// Codificação: caracteres de C₁ e C₂ estão sempre no alfabeto definido.
    /// Esta invariante é o que permite garantir que o campo de transporte
    /// resultante satisfaz `[a-zA-Z0-9]{26,35}`.
    #[test]
    fn encoded_chars_are_always_alphanumeric(s in seed_strategy(), t in counter_strategy()) {
        let (c1, c2) = derive_pair(&s, t);
        prop_assert!(c1.as_str().bytes().all(|b| b.is_ascii_alphanumeric()));
        prop_assert!(c2.as_str().bytes().all(|b| b.is_ascii_alphanumeric()));
        prop_assert_eq!(c1.as_str().len(), C1_TRANSPORT_LEN);
        prop_assert_eq!(c2.as_str().len(), C2_TRANSPORT_LEN);
    }

    /// Tempo constante semântico: verify_c2 retorna true sse os C₂ são iguais.
    /// (A propriedade de timing per se é validada por construção via subtle::ConstantTimeEq;
    /// aqui validamos a equivalência funcional.)
    #[test]
    fn verify_c2_equiv_to_equality(
        s1 in seed_strategy(),
        s2 in seed_strategy(),
        t1 in counter_strategy(),
        t2 in counter_strategy(),
    ) {
        let (_, a) = derive_pair(&s1, t1);
        let (_, b) = derive_pair(&s2, t2);
        let semantically_equal = a.as_str() == b.as_str();
        prop_assert_eq!(verify_c2(&a, &b), semantically_equal);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Propriedades — campo de transporte
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    /// Round-trip: encode → parse devolve exatamente os componentes originais.
    #[test]
    fn encode_parse_roundtrip(sid in seed_id_strategy(), s in seed_strategy(), t in counter_strategy()) {
        let (c1, _) = derive_pair(&s, t);
        let field = encode(&sid, &c1);
        let parsed = parse(&field).unwrap();
        prop_assert_eq!(parsed.seed_id, sid);
        prop_assert_eq!(parsed.c1, c1);
    }

    /// Encode sempre produz strings de tamanho fixo 35 dentro da faixa do PIX txid.
    #[test]
    fn encoded_field_length_is_35(sid in seed_id_strategy(), s in seed_strategy(), t in counter_strategy()) {
        let (c1, _) = derive_pair(&s, t);
        let field = encode(&sid, &c1);
        prop_assert_eq!(field.len(), 35);
        prop_assert!(field.bytes().all(|b| b.is_ascii_alphanumeric()));
    }

    /// Robustez do parser: qualquer entrada de tamanho até 50 bytes deve
    /// retornar Result sem panic. (Defesa contra panic em bordas de
    /// processamento — viola política de não-pânico da spec, Bloco 1.3.)
    #[test]
    fn parse_never_panics_on_arbitrary_strings(s in "\\PC{0,50}") {
        let _ = parse(&s);  // basta não panicar — resultado é irrelevante.
    }

    /// is_protocol_field é consistente com encode: tudo que encode produz
    /// é reconhecido por is_protocol_field.
    #[test]
    fn encoded_is_recognized_as_protocol(sid in seed_id_strategy(), s in seed_strategy(), t in counter_strategy()) {
        let (c1, _) = derive_pair(&s, t);
        let field = encode(&sid, &c1);
        prop_assert!(is_protocol_field(&field));
    }

    /// Prefixo errado: qualquer alteração do prefixo invalida a triagem.
    #[test]
    fn fields_with_wrong_prefix_rejected(
        sid in seed_id_strategy(),
        s in seed_strategy(),
        t in counter_strategy(),
        prefix in "[A-Z]{8}",
    ) {
        prop_assume!(prefix != PROTOCOL_PREFIX);
        let (c1, _) = derive_pair(&s, t);
        let mut field = encode(&sid, &c1);
        // Substitui o prefixo
        field.replace_range(..8, &prefix);
        prop_assert!(!is_protocol_field(&field));
        let result = parse(&field);
        prop_assert!(result.is_err());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Propriedades — assinatura
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    /// parse_sums_line nunca panica em entrada arbitrária — robustez contra
    /// SHA256SUMS malformados injetados por atacante.
    #[test]
    fn parse_sums_line_never_panics(s in "\\PC{0,200}") {
        let _ = parse_sums_line(&s);
    }

    /// verify_combined nunca panica em entradas arbitrárias dos parâmetros
    /// que vêm "de fora" (sums, signature, hash hex).
    #[test]
    fn verify_combined_never_panics(
        sums in prop::collection::vec(any::<u8>(), 0..=500),
        sig in prop::collection::vec(any::<u8>(), 0..=100),
        fname in "[a-z._]{0,50}",
        hex in "[0-9a-f]{0,80}",
    ) {
        let _ = verify_combined(&sums, &sig, RELEASE_PUBKEY, &fname, &hex);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sanity check: rejeições nunca confundem-se com `Verified`
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    /// Para entradas aleatórias o resultado nunca pode ser `Verified` —
    /// confirmaria forjamento gratuito de assinatura.
    #[test]
    fn random_bytes_never_verify(
        sums in prop::collection::vec(any::<u8>(), 1..=500),
        sig in prop::collection::vec(any::<u8>(), 64..=64),
        fname in "[a-z._]{1,50}",
        hex in "[0-9a-f]{64,64}",
    ) {
        if let Ok(SignatureCheck::Verified) =
            verify_combined(&sums, &sig, RELEASE_PUBKEY, &fname, &hex)
        {
            prop_assert!(false, "random input verified — forgery!");
        }
    }
}
