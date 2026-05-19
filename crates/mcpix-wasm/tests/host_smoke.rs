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

// ─────────────────────────────────────────────────────────────────────────
// Modo timestamp quantizado (S23) — exposto ao JS via
// `WasmDemo::new_quantized(window_seconds, initial_time_secs)`.
// O JS chama `tick(now_secs)` antes de cada operação. Este teste
// exercita o mesmo fluxo via Rust direto.
// ─────────────────────────────────────────────────────────────────────────

use mcpix_core::error::McpixError;
use mcpix_core::traits::Clock;
use mcpix_receiver_sdk::timestamp_counter::TimestampQuantizedCounter;
use std::sync::atomic::{AtomicU64, Ordering};

struct TestClock {
    now: AtomicU64,
}
impl Clock for TestClock {
    fn now_unix_secs(&self) -> u64 {
        self.now.load(Ordering::Relaxed)
    }
}

fn quantized_sdk(
    window_seconds: u64,
    initial: u64,
) -> (Arc<InMemorySeedStore>, ReceiverSdk, Arc<TestClock>) {
    let store = Arc::new(InMemorySeedStore::new());
    let clock = Arc::new(TestClock {
        now: AtomicU64::new(initial),
    });
    let counter = Arc::new(TimestampQuantizedCounter::with_window(
        clock.clone(),
        window_seconds,
    ));
    let rng = Arc::new(OsRandom);
    let sdk = ReceiverSdk::new(store.clone(), counter, rng);
    (store, sdk, clock)
}

#[test]
fn quantized_mode_rejects_same_window_double_generate() {
    // Dois generate_charge no mesmo quantum devem produzir
    // CounterCollision — defesa que evita C₂ duplicado silencioso.
    let (_, sdk, clock) = quantized_sdk(30, 1_700_000_000);
    let sid = SeedId::new("R1").unwrap();
    sdk.register(sid.clone()).unwrap();

    // Primeira: passa.
    let c1 = sdk.generate_charge(&sid, 100).unwrap();
    assert!(c1.counter > 0);

    // Mesmo quantum (clock não avançou): colisão.
    let err = sdk.generate_charge(&sid, 200).unwrap_err();
    match err {
        McpixError::CounterCollision { window_seconds } => assert_eq!(window_seconds, 30),
        other => panic!("expected CounterCollision, got {other:?}"),
    }

    // Avança para próximo quantum: passa.
    clock.now.store(1_700_000_000 + 30, Ordering::Relaxed);
    let c2 = sdk.generate_charge(&sid, 300).unwrap();
    assert!(c2.counter > c1.counter);
}

#[test]
fn quantized_mode_current_quantum_reflects_clock() {
    let (_, _sdk, clock) = quantized_sdk(30, 1_700_000_000);
    assert_eq!(clock.now_unix_secs() / 30, 56_666_666);

    clock.now.store(1_700_000_000 + 90, Ordering::Relaxed);
    assert_eq!(clock.now_unix_secs() / 30, 56_666_669);
}
