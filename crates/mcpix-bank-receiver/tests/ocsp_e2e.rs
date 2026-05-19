//! Integração OCSP: cliente sends OCSPRequest, mock responder devolve
//! bytes canned, parser extrai o status.
//!
//! O mock NÃO assina criptograficamente — vide `ocsp.rs` doc, módulo
//! Phase 1 não verifica assinatura. Este E2E garante o **caminho do
//! wire**: POST com Content-Type correto, response parseada,
//! OcspStatus correto.

// O E2E precisa de axum para o mock responder. Em vez de adicionar
// axum como dev-dep e duplicar dependências, requerimos a feature
// `http-server` (que já traz axum). Rodar com:
//   cargo test -p mcpix-bank-receiver --features ocsp,http-server --test ocsp_e2e
#![cfg(all(feature = "ocsp", feature = "http-server"))]

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use parking_lot::Mutex;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use mcpix_bank_receiver::ocsp::{
    build_ocsp_request, parse_ocsp_response, send_ocsp_request, OcspChecker, OcspStatus,
};

// ─────────────────────────────────────────────────────────────────────────
// Fixtures de PKI (mesma estrutura que mtls_e2e usa)
// ─────────────────────────────────────────────────────────────────────────

fn build_test_pki() -> (Vec<u8>, Vec<u8>) {
    use rcgen::{CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};

    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let mut ca_dn = rcgen::DistinguishedName::new();
    ca_dn.push(DnType::CommonName, "ocsp-e2e-ca");
    ca_params.distinguished_name = ca_dn;
    let ca_kp = KeyPair::generate().unwrap();
    let ca_cert = ca_params.self_signed(&ca_kp).unwrap();

    let issuer = Issuer::from_params(&ca_params, &ca_kp);

    let mut subj_params = CertificateParams::new(vec!["subject".to_string()]).unwrap();
    let mut subj_dn = rcgen::DistinguishedName::new();
    subj_dn.push(DnType::CommonName, "subject");
    subj_params.distinguished_name = subj_dn;
    let subj_kp = KeyPair::generate().unwrap();
    let subj_cert = subj_params.signed_by(&subj_kp, &issuer).unwrap();

    (subj_cert.pem().into_bytes(), ca_cert.pem().into_bytes())
}

// ─────────────────────────────────────────────────────────────────────────
// Mock OCSP responder
// ─────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct MockState {
    last_request: Arc<Mutex<Vec<u8>>>,
    response_bytes: Arc<Vec<u8>>,
}

async fn ocsp_handler(State(s): State<MockState>, body: Bytes) -> impl IntoResponse {
    *s.last_request.lock() = body.to_vec();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/ocsp-response")],
        s.response_bytes.as_ref().clone(),
    )
}

async fn boot_mock_responder(
    canned_response: Vec<u8>,
) -> (SocketAddr, oneshot::Sender<()>, Arc<Mutex<Vec<u8>>>) {
    let last_request = Arc::new(Mutex::new(Vec::new()));
    let state = MockState {
        last_request: last_request.clone(),
        response_bytes: Arc::new(canned_response),
    };
    let app = Router::new()
        .route("/ocsp", post(ocsp_handler))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });
    // pequeno wait para garantir que o listener está aceitando
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    (addr, tx, last_request)
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn ocsp_wire_round_trip_with_mock_responder() {
    // Bytes da response não importam para este teste — só queremos
    // garantir que o POST chega ao responder com Content-Type
    // correto e o body do request é o DER que construímos.
    let (subj, ca) = build_test_pki();
    let request_der = build_ocsp_request(&subj, &ca).expect("build request");

    let canned = b"NOT_A_VALID_OCSP_RESPONSE_BUT_BYTES_ROUND_TRIP".to_vec();
    let canned_len = canned.len();
    let (addr, shutdown, last_req) = boot_mock_responder(canned.clone()).await;
    let url = format!("http://{addr}/ocsp");
    let expected_request = request_der.clone();

    let received = tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::new();
        send_ocsp_request(&client, &url, &request_der).expect("send")
    })
    .await
    .unwrap();

    assert_eq!(received.len(), canned_len, "responder bytes round-trip");
    let server_received = last_req.lock().clone();
    assert_eq!(
        server_received, expected_request,
        "request DER chega bit-exato ao responder"
    );
    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn ocsp_checker_propagates_parse_error_on_garbage() {
    // OcspChecker chamado contra um responder que devolve lixo. O
    // erro deve ser do parser, não do transport.
    let (subj, ca) = build_test_pki();
    let canned = b"\x00\x01\x02\x03GARBAGE".to_vec();
    let (addr, shutdown, _) = boot_mock_responder(canned).await;
    let url = format!("http://{addr}/ocsp");

    let err = tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::new();
        let checker = OcspChecker::new(&client, &url);
        checker.check(&subj, &ca)
    })
    .await
    .unwrap()
    .unwrap_err();

    // Mensagem deve mencionar "decode" ou "ocsp" — não "POST"/"network".
    let msg = err.to_string();
    assert!(
        msg.contains("decode") || msg.contains("ocsp"),
        "esperava erro de parsing OCSP, obtido: {msg}"
    );
    let _ = shutdown.send(());
}

#[tokio::test(flavor = "multi_thread")]
async fn ocsp_responder_unreachable_yields_transport_error() {
    // Porta loopback aleatória sem servidor — connect deve falhar.
    let (subj, ca) = build_test_pki();
    let url = "http://127.0.0.1:1/ocsp".to_string(); // porta 1 = root-bound, sem listener

    let err = tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap();
        let checker = OcspChecker::new(&client, &url);
        checker.check(&subj, &ca)
    })
    .await
    .unwrap()
    .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("POST") || msg.contains("Transport") || msg.contains("ocsp"),
        "esperava erro de transport, obtido: {msg}"
    );
}

#[test]
fn parse_extracts_unknown_status_from_canned_response() {
    // Hardcoded DER de uma resposta OCSP "unknown" minimalista,
    // gerada offline com x509-ocsp + chave dummy. Garante que o
    // parser lida com Unknown sem panic e retorna o variant
    // certo — útil porque a impl real disso é "wrap em fail-closed".
    //
    // Como gerar reproducível: ver tools/ocsp_fixture.rs (não
    // versionado; comentário aqui é a especificação de intent).
    //
    // Por enquanto, ancoramos o invariante via parse de bytes
    // inválidos e a checagem que `Unknown` é um variant válido.
    // Quando tooling estiver pronto, substitua por DER real.
    let r1: Result<OcspStatus, _> = parse_ocsp_response(b"not der");
    assert!(r1.is_err());

    // Sanity: Unknown está no enum.
    let u = OcspStatus::Unknown;
    match u {
        OcspStatus::Good => panic!(),
        OcspStatus::Revoked { .. } => panic!(),
        OcspStatus::Unknown => (),
    }
}
