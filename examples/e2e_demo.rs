//! Demo end-to-end CLI dos três módulos lado a lado.
//!
//! Executar: `cargo run -p mcpix-examples --bin e2e_demo`

use std::sync::Arc;

use mcpix_bank_payer_mock::{PayerBankMock, PaymentRequest};
use mcpix_bank_receiver::{BankReceiver, InMemoryBankReceiver, Requester};
use mcpix_core::state::ValidationOutcome;
use mcpix_core::traits::SeedStore;
use mcpix_core::types::{Seed, SeedId};
use mcpix_receiver_sdk::{
    memory_store::InMemorySeedStore, monotonic_counter::InMemoryCounter,
    system_random::OsRandom, ReceiverSdk,
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

    println!("\n────── demo concluída ──────");
}
