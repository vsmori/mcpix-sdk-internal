//! Smoke do fluxo da demo executado no host (rlib).
//!
//! Mesma API que o JS consome — diferença é só o ABI (`JsValue` vira
//! tipo Rust direto). Este teste verifica que:
//!   1. `register → generate_charge → payer_recover_c2` produz C₂
//!      bit-exato igual ao retained_c2 do recebedor (substituição
//!      institucional);
//!   2. `validate_receipt` aprova → marca consumido → segunda chamada
//!      retorna Replay;
//!   3. campo de transporte alterado → Mismatch.
//!
//! Sem este teste, regressões na API wasm só apareceriam num browser.

#![cfg(not(target_arch = "wasm32"))]

use mcpix_core::state::{apply_recover_c2, ValidationOutcome};
use mcpix_core::traits::SeedStore;
use mcpix_core::transport_field;
use mcpix_core::types::SeedId;
use mcpix_receiver_sdk::memory_store::InMemorySeedStore;
use mcpix_receiver_sdk::monotonic_counter::InMemoryCounter;
use mcpix_receiver_sdk::system_random::OsRandom;
use mcpix_receiver_sdk::ReceiverSdk;
use std::sync::Arc;

fn fresh() -> (Arc<InMemorySeedStore>, ReceiverSdk) {
    let store = Arc::new(InMemorySeedStore::new());
    let counter = Arc::new(InMemoryCounter::new());
    let rng = Arc::new(OsRandom);
    let sdk = ReceiverSdk::new(store.clone(), counter, rng);
    (store, sdk)
}

#[test]
fn demo_flow_recovers_same_c2_bit_exact() {
    let (store, sdk) = fresh();
    let sid = SeedId::new("R1").unwrap();
    let proof = sdk.register(sid.clone()).unwrap();
    let charge = sdk.generate_charge(&proof.seed_id, 9900).unwrap();

    // Recebedor: lê o retido para mostrar na UI.
    let retained = sdk
        .peek_retained(&proof.seed_id, charge.counter)
        .unwrap()
        .unwrap();

    // Pagador: parseia o campo, consulta a semente no recebedor,
    // reconstrói C₂.
    let parsed = transport_field::parse(&charge.transport_field).unwrap();
    let seed = store.get_seed(&parsed.seed_id).unwrap().unwrap();
    let recovered = apply_recover_c2(&seed, charge.counter, &parsed.c1);

    assert_eq!(retained.expected_c2.as_str(), recovered.as_str());
}

#[test]
fn demo_flow_rejects_replay() {
    let (store, sdk) = fresh();
    let sid = SeedId::new("R1").unwrap();
    sdk.register(sid.clone()).unwrap();
    let charge = sdk.generate_charge(&sid, 100).unwrap();

    let parsed = transport_field::parse(&charge.transport_field).unwrap();
    let seed = store.get_seed(&parsed.seed_id).unwrap().unwrap();
    let c2 = apply_recover_c2(&seed, charge.counter, &parsed.c1);
    let c2_str = c2.as_str().to_string();

    assert_eq!(
        sdk.validate_receipt(&sid, charge.counter, &c2_str).unwrap(),
        ValidationOutcome::Valid
    );
    assert_eq!(
        sdk.validate_receipt(&sid, charge.counter, &c2_str).unwrap(),
        ValidationOutcome::Replay
    );
}

#[test]
fn demo_flow_rejects_tampered_transport_field() {
    let (store, sdk) = fresh();
    let sid = SeedId::new("R1").unwrap();
    sdk.register(sid.clone()).unwrap();
    let charge = sdk.generate_charge(&sid, 100).unwrap();

    // Flip um char no C1 (últimos 11 chars do campo). Mantém alfanumérico
    // para sobreviver ao gate de charset do parser.
    let mut bytes = charge.transport_field.as_bytes().to_vec();
    let last = bytes.len() - 1;
    bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
    let mangled = String::from_utf8(bytes).unwrap();

    let parsed = transport_field::parse(&mangled).unwrap();
    let seed = store.get_seed(&parsed.seed_id).unwrap().unwrap();
    let c2 = apply_recover_c2(&seed, charge.counter, &parsed.c1);

    // O C2 derivado a partir do C1 alterado é diferente do retido →
    // validação falha como Mismatch.
    assert_eq!(
        sdk.validate_receipt(&sid, charge.counter, c2.as_str())
            .unwrap(),
        ValidationOutcome::Mismatch
    );
}
