//! Integração mTLS fim-a-fim.
//!
//! Esta suite **gera certificados in-process** (via `rcgen`) para evitar
//! depender de arquivos externos. Estrutura:
//!
//! ```text
//!   CA raiz (auto-assinada)
//!     ├── server.crt   (SAN: DNS=localhost, IP=127.0.0.1)
//!     └── client.crt   (SAN: URI=urn:mcpix:institution:BANK_PAYER)
//! ```
//!
//! Cenários cobertos:
//!  1. Cliente com cert válido + servidor mTLS → handshake OK, fluxo normal.
//!  2. Cliente SEM cert → handshake rejeitado pela camada TLS.
//!  3. Cliente com cert de OUTRA CA (não confiada) → handshake rejeitado.

#![cfg(feature = "mtls")]

use std::net::SocketAddr;
use std::sync::Arc;

use mcpix_bank_receiver::http_client::HttpBankReceiver;
use mcpix_bank_receiver::mtls::extract_institution_id;
use mcpix_bank_receiver::mtls_client::{build_mtls_client, MtlsClientMaterial};
use mcpix_bank_receiver::mtls_server::{build_server_config, serve_mtls};
use mcpix_bank_receiver::{BankReceiver, InMemoryBankReceiver, Requester};
use mcpix_core::types::{Seed, SeedId};

struct Pki {
    ca_pem: Vec<u8>,
    server_cert_pem: Vec<u8>,
    server_key_pem: Vec<u8>,
    client_cert_pem: Vec<u8>,
    client_key_pem: Vec<u8>,
    client_cert_der: Vec<u8>,
}

fn build_pki() -> Pki {
    use rcgen::{
        CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose, SanType,
    };

    // ── CA raiz auto-assinada ───────────────────────────────────────────
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::CrlSign,
    ];
    let mut ca_dn = rcgen::DistinguishedName::new();
    ca_dn.push(DnType::CommonName, "mcpix-test-ca");
    ca_params.distinguished_name = ca_dn;
    let ca_kp = KeyPair::generate().unwrap();
    let ca_cert = ca_params.self_signed(&ca_kp).unwrap();
    let ca_pem = ca_cert.pem().into_bytes();

    // `Issuer` é o handle que `signed_by` consome para assinar filhos.
    let issuer = Issuer::from_params(&ca_params, &ca_kp);

    // ── Server cert (SAN: DNS=localhost, IP=127.0.0.1) ─────────────────
    let mut srv_params =
        CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    srv_params.subject_alt_names = vec![
        SanType::DnsName("localhost".try_into().unwrap()),
        SanType::IpAddress("127.0.0.1".parse().unwrap()),
    ];
    srv_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let mut srv_dn = rcgen::DistinguishedName::new();
    srv_dn.push(DnType::CommonName, "mcpix-bank-receiver");
    srv_params.distinguished_name = srv_dn;
    let srv_kp = KeyPair::generate().unwrap();
    let srv_cert = srv_params.signed_by(&srv_kp, &issuer).unwrap();
    let server_cert_pem = srv_cert.pem().into_bytes();
    let server_key_pem = srv_kp.serialize_pem().into_bytes();

    // ── Client cert (SAN URI = institution_id) ─────────────────────────
    let mut cli_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    cli_params.subject_alt_names = vec![SanType::URI(
        "urn:mcpix:institution:BANK_PAYER"
            .try_into()
            .unwrap(),
    )];
    cli_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    let mut cli_dn = rcgen::DistinguishedName::new();
    cli_dn.push(DnType::CommonName, "bank-payer");
    cli_params.distinguished_name = cli_dn;
    let cli_kp = KeyPair::generate().unwrap();
    let cli_cert = cli_params.signed_by(&cli_kp, &issuer).unwrap();
    let client_cert_pem = cli_cert.pem().into_bytes();
    let client_cert_der = cli_cert.der().to_vec();
    let client_key_pem = cli_kp.serialize_pem().into_bytes();

    Pki {
        ca_pem,
        server_cert_pem,
        server_key_pem,
        client_cert_pem,
        client_key_pem,
        client_cert_der,
    }
}

async fn boot_mtls_server(
    pki: &Pki,
) -> (SocketAddr, mcpix_bank_receiver::mtls_server::ServerHandle) {
    let bank: Arc<dyn BankReceiver> = Arc::new(InMemoryBankReceiver::new());
    // Pre-popula uma semente conhecida para os testes de leitura.
    bank.register_seed(&SeedId::new("R1").unwrap(), Seed::from_bytes([0x42; 32]))
        .unwrap();

    let cfg = build_server_config(&pki.server_cert_pem, &pki.server_key_pem, &pki.ca_pem)
        .expect("server config");

    serve_mtls("127.0.0.1:0".parse().unwrap(), bank, cfg)
        .await
        .expect("mtls server start")
}

#[tokio::test(flavor = "multi_thread")]
async fn mtls_round_trip_succeeds_with_valid_client_cert() {
    let pki = build_pki();
    let (addr, handle) = boot_mtls_server(&pki).await;
    let base = format!("https://localhost:{}", addr.port());

    // Mapeia "localhost" para o IP local — necessário porque o servidor está
    // em 127.0.0.1:<random> e o cert do servidor tem SAN DNS=localhost.
    let resolved = addr;
    let mat = MtlsClientMaterial {
        client_cert_pem: pki.client_cert_pem.clone(),
        client_key_pem: pki.client_key_pem.clone(),
        server_ca_pem: pki.ca_pem.clone(),
    };

    // O helper `build_mtls_client` cobre o caso geral; aqui precisamos
    // adicionar `.resolve("localhost", addr)` porque o servidor está em uma
    // porta aleatória de loopback. Reconstruímos manualmente. Para uso
    // produtivo o helper basta.
    let outcome = tokio::task::spawn_blocking(move || -> Result<Seed, String> {
        let _ = build_mtls_client(&mat).map_err(|e| e.to_string())?; // sanity
        let mut id = mat.client_cert_pem.clone();
        id.push(b'\n');
        id.extend_from_slice(&mat.client_key_pem);
        let client = reqwest::blocking::ClientBuilder::new()
            .use_rustls_tls()
            .add_root_certificate(
                reqwest::Certificate::from_pem(&mat.server_ca_pem).map_err(|e| e.to_string())?,
            )
            .identity(reqwest::Identity::from_pem(&id).map_err(|e| e.to_string())?)
            .resolve("localhost", resolved)
            .build()
            .map_err(|e| e.to_string())?;
        let http_bank = HttpBankReceiver::with_client(base, client);
        http_bank
            .lookup_seed(
                &SeedId::new("R1").unwrap(),
                &Requester {
                    institution_id: "BANK_PAYER".into(),
                },
            )
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap();

    let seed = outcome.expect("lookup should succeed under mTLS");
    assert_eq!(seed.as_bytes(), &[0x42u8; 32]);

    handle.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn mtls_rejects_client_without_cert() {
    let pki = build_pki();
    let (addr, handle) = boot_mtls_server(&pki).await;
    let resolved = addr;
    let ca = pki.ca_pem.clone();
    let base = format!("https://localhost:{}", addr.port());

    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let client = reqwest::blocking::ClientBuilder::new()
            .use_rustls_tls()
            .add_root_certificate(reqwest::Certificate::from_pem(&ca).unwrap())
            // *sem* .identity() — não apresenta client cert
            .resolve("localhost", resolved)
            .build()
            .map_err(|e| e.to_string())?;
        client
            .get(format!("{base}/v1/healthz"))
            .send()
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap();

    // Sem client cert, o servidor rustls aborta o handshake mTLS antes de
    // responder. O reqwest pode reportar como "connection closed",
    // "handshake failure" ou "error sending request" dependendo da versão.
    // Basta confirmarmos que NÃO houve sucesso.
    assert!(
        result.is_err(),
        "expected TLS handshake failure, got Ok response"
    );

    handle.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn mtls_rejects_client_from_untrusted_ca() {
    let pki = build_pki();
    let (addr, handle) = boot_mtls_server(&pki).await;
    let resolved = addr;
    let trusted_ca = pki.ca_pem.clone();
    let base = format!("https://localhost:{}", addr.port());

    // Gera outra CA + cliente assinado por ela — não confiada pelo servidor.
    let attacker = build_pki();

    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut id = attacker.client_cert_pem.clone();
        id.push(b'\n');
        id.extend_from_slice(&attacker.client_key_pem);

        let client = reqwest::blocking::ClientBuilder::new()
            .use_rustls_tls()
            .add_root_certificate(reqwest::Certificate::from_pem(&trusted_ca).unwrap())
            .identity(reqwest::Identity::from_pem(&id).map_err(|e| e.to_string())?)
            .resolve("localhost", resolved)
            .build()
            .map_err(|e| e.to_string())?;
        client
            .get(format!("{base}/v1/healthz"))
            .send()
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap();

    assert!(
        result.is_err(),
        "expected handshake failure for cert from untrusted CA"
    );

    handle.shutdown();
}

#[test]
fn extract_identity_from_client_cert_der() {
    // Sanity: o helper de extração casa com o cert que produzimos.
    let pki = build_pki();
    let id = extract_institution_id(&pki.client_cert_der).unwrap();
    assert_eq!(id, "BANK_PAYER");
}
