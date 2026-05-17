//! Demo end-to-end CLI dos três módulos lado a lado.
//!
//! Executar: `cargo run -p mcpix-examples --bin e2e_demo`

use std::sync::Arc;

use mcpix_bank_payer_mock::{PayerBankMock, PaymentRequest, PaymentRequestWindowed};
use mcpix_bank_receiver::{BankReceiver, InMemoryBankReceiver, Requester};
use mcpix_core::state::ValidationOutcome;
use mcpix_core::traits::SeedStore;
use mcpix_core::types::{Seed, SeedId};
use mcpix_receiver_sdk::{
    clock::TestClock, memory_store::InMemorySeedStore, monotonic_counter::InMemoryCounter,
    system_random::OsRandom, timestamp_counter::TimestampQuantizedCounter, ReceiverSdk,
};

fn step(n: usize, title: &str) {
    println!("\n────── PASSO {n}: {title} ──────");
}

fn main() {
    println!("┌──────────────────────────────────────────────────┐");
    println!("│  mcpix-sdk — demonstração end-to-end (Rust pure) │");
    println!("└──────────────────────────────────────────────────┘");

    // ──────────────── infraestrutura compartilhada ────────────────
    let local_store = Arc::new(InMemorySeedStore::new());
    let counter = Arc::new(InMemoryCounter::new());
    let rng = Arc::new(OsRandom);
    let receiver_sdk = ReceiverSdk::new(local_store.clone(), counter, rng);

    let receiver_bank = InMemoryBankReceiver::new();

    // ──────────────── PASSO 1: recebedor se cadastra ────────────────
    step(1, "recebedor cadastra-se localmente e no banco recebedor");
    let seed_id = SeedId::new("R1").unwrap();
    let proof = receiver_sdk.register(seed_id.clone()).unwrap();
    println!("• recebedor local: SeedId = {}", proof.seed_id.as_str());

    // Em produção o banco recebedor recebe a semente via canal autenticado
    // (mTLS + key wrap). Aqui pegamos do SeedStore só para a demo funcionar.
    let seed_for_bank = local_store
        .clone()
        .as_ref()
        .get_seed(&seed_id)
        .unwrap()
        .unwrap_or_else(|| Seed::from_bytes([0; 32]));
    receiver_bank.register_seed(&seed_id, seed_for_bank).unwrap();
    println!("• banco recebedor: semente custodiada (interface preparada p/ HSM)");

    // ──────────────── PASSO 2: gera cobrança ────────────────
    step(2, "recebedor (offline) gera par atômico (C₁, C₂) e expõe instrumento");
    let charge = receiver_sdk.generate_charge(&seed_id, 9900).unwrap();
    let retained = receiver_sdk
        .peek_retained(&seed_id, charge.counter)
        .unwrap()
        .unwrap();
    println!("• campo de transporte (público): {}", charge.transport_field);
    println!("  └─ tamanho: {} chars (alvo 26..=35)", charge.transport_field.len());
    println!("• C₂ retido localmente (secreto): {}", retained.expected_c2.as_str());
    println!("• counter usado: T = {}", charge.counter);
    println!("• amount_cents: {}", charge.amount_cents);

    // ──────────────── PASSO 3: pagador entrega ao banco do pagador ────────────────
    step(3, "banco do pagador identifica protocolo, recompõe C₂ via lookup");
    let payer_bank = PayerBankMock::new(&receiver_bank);
    let receipt = payer_bank
        .process_payment(PaymentRequest {
            instrument_string: &charge.transport_field,
            amount_cents: charge.amount_cents,
            counter: charge.counter,
            requester: Requester { institution_id: "BANK_PAYER".into() },
        })
        .unwrap();
    println!("• comprovante emitido pelo banco do pagador:");
    println!("  ├─ identifier (C₂ recomposto): {}", receipt.identifier);
    println!("  ├─ amount_cents:               {}", receipt.amount_cents);
    println!("  └─ nota:                       {}", receipt.note);

    // ──────────────── PASSO 4: recebedor valida ────────────────
    step(4, "recebedor (ainda offline) compara C₂ apresentado em tempo constante");
    let outcome = receiver_sdk
        .validate_receipt(&seed_id, charge.counter, &receipt.identifier)
        .unwrap();
    assert_eq!(outcome, ValidationOutcome::Valid);
    println!("• resultado: ✓ VALID — BIP visual emitido");

    // ──────────────── PASSO 5: replay deve falhar ────────────────
    step(5, "replay rejeitado: mesmo C₂ apresentado uma segunda vez");
    let replay = receiver_sdk
        .validate_receipt(&seed_id, charge.counter, &receipt.identifier)
        .unwrap();
    assert_eq!(replay, ValidationOutcome::Replay);
    println!("• resultado: ✗ REPLAY — defesa de uso único acionada");

    // ──────────────── PARTE B: modo timestamp quantizado ────────────────
    println!("\n══════════════════ PARTE B ══════════════════");
    println!("Mesma demo com T = timestamp quantizado (RFC 6238 style)");
    println!("Janela: 30s. Recebedor e banco compartilham relógio para a demo.");

    // Clock determinístico — ambos os lados leem do mesmo TestClock.
    let clock = Arc::new(TestClock::new(1_700_000_010));
    let q_counter = Arc::new(TimestampQuantizedCounter::with_window(clock.clone(), 30));
    let q_store = Arc::new(InMemorySeedStore::new());
    let q_sdk = ReceiverSdk::new(q_store.clone(), q_counter.clone(), Arc::new(OsRandom));

    let q_sid = SeedId::new("R2").unwrap();
    q_sdk.register(q_sid.clone()).unwrap();
    let q_seed_for_bank = q_store.get_seed(&q_sid).unwrap().unwrap();
    receiver_bank.register_seed(&q_sid, q_seed_for_bank).unwrap();

    step(6, "recebedor gera cobrança no quantum atual (T = now/30)");
    let q_charge = q_sdk.generate_charge(&q_sid, 4200).unwrap();
    // Acesso ao TestClock via método inerente (não trait), evitando ambiguidade.
    let now = mcpix_core::traits::Clock::now_unix_secs(clock.as_ref());
    println!("• T usado: {} (= {} / 30)", q_charge.counter, now);
    println!("• campo de transporte: {}", q_charge.transport_field);

    step(7, "tentativa de 2ª cobrança no mesmo quantum: rejeitada");
    let err = q_sdk.generate_charge(&q_sid, 100).unwrap_err();
    println!("• erro retornado: {err}");

    step(8, "avança o relógio +30s → próximo quantum → 2ª cobrança OK");
    clock.advance(30);
    let q_charge2 = q_sdk.generate_charge(&q_sid, 100).unwrap();
    println!("• T usado: {} (após advance)", q_charge2.counter);
    assert_eq!(q_charge2.counter, q_charge.counter + 1);

    step(9, "banco pagador processa com tolerância de ±1 janela (drift simulado)");
    // Simulamos drift: banco tem clock 10s atrás do recebedor mas dentro do quantum.
    let payer_bank_q = PayerBankMock::new(&receiver_bank);
    let receipt_w = payer_bank_q
        .process_payment_windowed(PaymentRequestWindowed {
            instrument_string: &q_charge.transport_field,
            amount_cents: 4200,
            current_quantum: q_charge.counter, // banco "concorda" com o quantum
            tolerance_windows: 1,
            requester: Requester { institution_id: "BANK_PAYER".into() },
        })
        .unwrap();
    println!("• candidatos emitidos pelo banco: {}", receipt_w.candidates.len());
    for (t, c2) in &receipt_w.candidates {
        let mark = if *t == q_charge.counter { "← match esperado" } else { "" };
        println!("  ├─ T={t}  →  C₂={c2}  {mark}");
    }

    step(10, "recebedor tenta cada candidato — primeiro match é o vencedor");
    let mut accepted = false;
    for (_, c2) in &receipt_w.candidates {
        if matches!(
            q_sdk.validate_receipt(&q_sid, q_charge.counter, c2).unwrap(),
            ValidationOutcome::Valid
        ) {
            println!("• ✓ VALID com C₂={c2}");
            accepted = true;
            break;
        }
    }
    assert!(accepted, "windowed flow should accept the matching candidate");

    println!("\n────── demo concluída ──────");
}
