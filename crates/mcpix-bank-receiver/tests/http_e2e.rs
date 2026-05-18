//! Integração HTTP fim-a-fim:
//!   1. Spawnia `http_server` em loopback porta aleatória
//!   2. Cliente HTTP registra + faz lookup
//!   3. Validação cruzada: cliente via trait `BankReceiver` produz mesmo
//!      resultado que chamada in-process à `InMemoryBankReceiver`
//!
//! Requer features `http-server` e `http-client` ligadas.

#![cfg(all(feature = "http-server", feature = "http-client"))]

use std::sync::Arc;

use mcpix_bank_receiver::http_client::HttpBankReceiver;
use mcpix_bank_receiver::http_server;
use mcpix_bank_receiver::{BankReceiver, InMemoryBankReceiver, Requester};
use mcpix_core::types::{Seed, SeedId};
use tokio::sync::oneshot;

async fn boot_server() -> (std::net::SocketAddr, oneshot::Sender<()>) {
    let bank: Arc<dyn BankReceiver> = Arc::new(InMemoryBankReceiver::new());
    let (tx, rx) = oneshot::channel::<()>();
    let addr = http_server::serve("127.0.0.1:0".parse().unwrap(), bank, async move {
        let _ = rx.await;
    })
    .await
    .expect("server failed to bind");
    (addr, tx)
}

#[tokio::test(flavor = "multi_thread")]
async fn http_round_trip_register_and_lookup() {
    let (addr, shutdown) = boot_server().await;
    let base = format!("http://{addr}");

    // Cliente HTTP sync — usamos spawn_blocking porque está em runtime async.
    let result = tokio::task::spawn_blocking(move || {
        let client = HttpBankReceiver::new(base);
        let sid = SeedId::new("R1").unwrap();
        let seed = Seed::from_bytes([0x77; 32]);

        client.register_seed(&sid, seed.clone()).unwrap();
        let got = client
            .lookup_seed(&sid, &Requester { institution_id: "PAYER".into() })
            .unwrap();
        got.as_bytes() == seed.as_bytes()
    })
    .await
    .unwrap();
    assert!(result);

    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn http_lookup_unknown_seed_returns_typed_error() {
    use mcpix_core::error::McpixError;
    let (addr, shutdown) = boot_server().await;
    let base = format!("http://{addr}");

    let outcome = tokio::task::spawn_blocking(move || {
        let client = HttpBankReceiver::new(base);
        client.lookup_seed(
            &SeedId::new("ghost").unwrap(),
            &Requester { institution_id: "x".into() },
        )
    })
    .await
    .unwrap();

    assert!(matches!(outcome, Err(McpixError::UnknownSeed)));
    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn full_protocol_through_http() {
    // Cenário end-to-end: recebedor registra semente no servidor → emite charge
    // → banco do pagador (que usa HttpBankReceiver) recupera C₂ via HTTP →
    // recebedor valida. Exercita o caminho real Recebedor → HTTP → Banco.
    use mcpix_bank_payer_mock::{PayerBankMock, PaymentRequest};
    use mcpix_core::state::{apply_generate_charge, GenerateChargeCommand, ValidationOutcome, apply_validate_receipt};

    let (addr, shutdown) = boot_server().await;
    let base = format!("http://{addr}");

    let outcome_ok = tokio::task::spawn_blocking(move || {
        let client = HttpBankReceiver::new(base);
        let sid = SeedId::new("R1").unwrap();
        let seed = Seed::from_bytes([0xAB; 32]);
        client.register_seed(&sid, seed.clone()).unwrap();

        // Recebedor offline produz par.
        let charge_out = apply_generate_charge(&seed, GenerateChargeCommand {
            seed_id: sid.clone(),
            counter: 7,
            amount_cents: 1234,
        });

        // Banco do pagador (com HttpBankReceiver injetado) processa.
        let payer = PayerBankMock::new(&client);
        let receipt = payer.process_payment(PaymentRequest {
            instrument_string: &charge_out.charge.transport_field,
            amount_cents: 1234,
            counter: 7,
            requester: Requester { institution_id: "PAYER_BANK".into() },
        }).unwrap();

        // Recebedor valida o que o banco devolveu.
        let presented = mcpix_core::types::C2::parse(&receipt.identifier).unwrap();
        apply_validate_receipt(&charge_out.retained, &presented)
    })
    .await
    .unwrap();

    assert_eq!(outcome_ok, ValidationOutcome::Valid);
    let _ = shutdown.send(());
}

// ─────────────────────────────────────────────────────────────────────────
// Capability negotiation (S17)
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn http_capabilities_reports_v1() {
    // O receiver in-memory usa o default da trait, que devolve tudo
    // que o build conhece — hoje só V1.
    let (addr, shutdown) = boot_server().await;
    let base = format!("http://{addr}");

    let versions = tokio::task::spawn_blocking(move || {
        let client = HttpBankReceiver::new(base);
        client.supported_versions().unwrap()
    })
    .await
    .unwrap();

    assert_eq!(
        versions,
        vec![mcpix_core::version::ProtocolVersion::V1],
        "default impl should advertise everything in ProtocolVersion::all()"
    );
    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn http_capabilities_payload_matches_wire_contract() {
    // Acessa o endpoint cru e verifica a forma exata do JSON. Se
    // alguém mudar `CapabilitiesPayload` para `{caps: [...]}` em vez
    // de `{versions: [...]}`, isso quebra aqui antes do peer real.
    let (addr, shutdown) = boot_server().await;
    let url = format!("http://{addr}/v1/capabilities");

    let body = tokio::task::spawn_blocking(move || {
        reqwest::blocking::get(url)
            .unwrap()
            .text()
            .unwrap()
    })
    .await
    .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    let versions = parsed["versions"].as_array().expect("versions array");
    assert!(versions.iter().any(|v| v == "PIXOFFv1"));
    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn negotiation_with_real_server_picks_v1() {
    // End-to-end: cliente faz capability call → roda negotiate sobre
    // resultado → pega V1. Demonstra o caminho recomendado para
    // payer banks no doc.
    let (addr, shutdown) = boot_server().await;
    let base = format!("http://{addr}");

    let agreed = tokio::task::spawn_blocking(move || {
        let client = HttpBankReceiver::new(base);
        // Esta lista deveria vir do request_response cru para o caminho
        // de "peer suporta versões que este build desconhece"; aqui
        // usamos a versão já filtrada porque cobre o caso comum.
        let peer = client.supported_versions().unwrap();
        let peer_as_strings: Vec<String> =
            peer.iter().map(|v| v.prefix().to_string()).collect();
        mcpix_core::version::negotiate_version(
            mcpix_core::version::ProtocolVersion::all(),
            &peer_as_strings,
        )
    })
    .await
    .unwrap();

    assert_eq!(agreed, Some(mcpix_core::version::ProtocolVersion::V1));
    let _ = shutdown.send(());
}
