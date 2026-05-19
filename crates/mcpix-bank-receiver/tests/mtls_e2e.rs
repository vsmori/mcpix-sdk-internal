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
use mcpix_bank_receiver::mtls_server::{
    build_server_config, build_server_config_full, serve_mtls, ServerTlsConfig,
};
use mcpix_bank_receiver::{BankReceiver, InMemoryBankReceiver, Requester};
use mcpix_core::types::{Seed, SeedId};

struct Pki {
    ca_pem: Vec<u8>,
    ca_params: rcgen::CertificateParams,
    ca_kp: rcgen::KeyPair,
    server_cert_pem: Vec<u8>,
    server_key_pem: Vec<u8>,
    server_serial: rcgen::SerialNumber,
    client_cert_pem: Vec<u8>,
    client_key_pem: Vec<u8>,
    client_cert_der: Vec<u8>,
    client_serial: rcgen::SerialNumber,
}

fn build_pki() -> Pki {
    use rcgen::{
        CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose,
        SanType, SerialNumber,
    };

    // Serials explícitos — necessários para construir CRLs apontando para
    // estes certs. Em produção a CA emite serial único; aqui usamos
    // valores fixos por test para previsibilidade.
    let server_serial = SerialNumber::from(0x1001u64);
    let client_serial = SerialNumber::from(0x2001u64);

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

    let issuer = Issuer::from_params(&ca_params, &ca_kp);

    // ── Server cert (SAN: DNS=localhost, IP=127.0.0.1) ─────────────────
    let mut srv_params = CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    srv_params.serial_number = Some(server_serial.clone());
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
    cli_params.serial_number = Some(client_serial.clone());
    cli_params.subject_alt_names = vec![SanType::URI(
        "urn:mcpix:institution:BANK_PAYER".try_into().unwrap(),
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
        ca_params,
        ca_kp,
        server_cert_pem,
        server_key_pem,
        server_serial,
        client_cert_pem,
        client_key_pem,
        client_cert_der,
        client_serial,
    }
}

/// Constrói uma CRL assinada pela CA do `pki`, revogando os serials
/// fornecidos. Janela `this_update..next_update` de 24h.
fn build_crl_pem(pki: &Pki, revoked_serials: &[rcgen::SerialNumber]) -> Vec<u8> {
    use rcgen::{
        CertificateRevocationListParams, Issuer, KeyIdMethod, RevocationReason, RevokedCertParams,
        SerialNumber,
    };
    use time::{Duration, OffsetDateTime};

    let now = OffsetDateTime::now_utc();
    let revoked_certs: Vec<RevokedCertParams> = revoked_serials
        .iter()
        .cloned()
        .map(|sn: SerialNumber| RevokedCertParams {
            serial_number: sn,
            revocation_time: now - Duration::minutes(1),
            reason_code: Some(RevocationReason::KeyCompromise),
            invalidity_date: None,
        })
        .collect();

    let issuer = Issuer::from_params(&pki.ca_params, &pki.ca_kp);
    let crl = CertificateRevocationListParams {
        this_update: now - Duration::minutes(1),
        next_update: now + Duration::hours(24),
        crl_number: SerialNumber::from(1u64),
        issuing_distribution_point: None,
        revoked_certs,
        key_identifier_method: KeyIdMethod::Sha256,
    }
    .signed_by(&issuer)
    .expect("crl sign");
    crl.pem().expect("crl pem").into_bytes()
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
    let mat = MtlsClientMaterial::new(
        pki.client_cert_pem.clone(),
        pki.client_key_pem.clone(),
        pki.ca_pem.clone(),
    );

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

// ─────────────────────────────────────────────────────────────────────────
// Testes de revogação via CRL (THREAT_MODEL §6.5)
// ─────────────────────────────────────────────────────────────────────────

async fn boot_mtls_server_with_crl(
    pki: &Pki,
    client_crls_pem: &[u8],
) -> (SocketAddr, mcpix_bank_receiver::mtls_server::ServerHandle) {
    let bank: Arc<dyn BankReceiver> = Arc::new(InMemoryBankReceiver::new());
    bank.register_seed(&SeedId::new("R1").unwrap(), Seed::from_bytes([0x42; 32]))
        .unwrap();

    let cfg = build_server_config_full(
        &ServerTlsConfig::new(&pki.server_cert_pem, &pki.server_key_pem, &pki.ca_pem)
            .with_client_crls(client_crls_pem),
    )
    .expect("server config with crl");
    serve_mtls("127.0.0.1:0".parse().unwrap(), bank, cfg)
        .await
        .expect("mtls server start")
}

#[tokio::test(flavor = "multi_thread")]
async fn mtls_rejects_revoked_client_cert() {
    let pki = build_pki();
    // CRL revogando o client cert do `pki`.
    let crl_pem = build_crl_pem(&pki, std::slice::from_ref(&pki.client_serial));
    let (addr, handle) = boot_mtls_server_with_crl(&pki, &crl_pem).await;
    let resolved = addr;
    let base = format!("https://localhost:{}", addr.port());

    let mat = MtlsClientMaterial::new(
        pki.client_cert_pem.clone(),
        pki.client_key_pem.clone(),
        pki.ca_pem.clone(),
    );

    // Apesar do cert ser válido (assinado pela CA), está revogado.
    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let _ = build_mtls_client(&mat).map_err(|e| e.to_string())?;
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
        "expected handshake failure for revoked client cert, got Ok"
    );

    handle.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn mtls_accepts_non_revoked_client_when_crl_active() {
    // Smoke negativo: CRL ativa, mas vazia → handshake deve passar.
    // Garante que o caminho "com CRL" não é estritamente mais restritivo
    // — só rejeita certs explicitamente listados.
    let pki = build_pki();
    let crl_pem = build_crl_pem(&pki, &[]);
    let (addr, handle) = boot_mtls_server_with_crl(&pki, &crl_pem).await;
    let resolved = addr;
    let base = format!("https://localhost:{}", addr.port());

    let mat = MtlsClientMaterial::new(
        pki.client_cert_pem.clone(),
        pki.client_key_pem.clone(),
        pki.ca_pem.clone(),
    );

    let outcome = tokio::task::spawn_blocking(move || -> Result<Seed, String> {
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

    let seed = outcome.expect("handshake should succeed with non-revoked cert under CRL");
    assert_eq!(seed.as_bytes(), &[0x42u8; 32]);

    handle.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn client_rejects_revoked_server_cert_via_crl() {
    // Dual: agora o cliente verifica o cert do servidor contra uma CRL
    // que revoga o server. CA confia no cert, mas a CRL o lista.
    let pki = build_pki();
    let crl_pem = build_crl_pem(&pki, std::slice::from_ref(&pki.server_serial));

    // Servidor sobe normalmente (não sabe que foi revogado pela CA — é a CA
    // que emite a CRL; o cliente é quem checa).
    let (addr, handle) = boot_mtls_server(&pki).await;
    let resolved = addr;
    let base = format!("https://localhost:{}", addr.port());

    let mat = MtlsClientMaterial::new(
        pki.client_cert_pem.clone(),
        pki.client_key_pem.clone(),
        pki.ca_pem.clone(),
    )
    .with_server_crls(crl_pem);

    let result = tokio::task::spawn_blocking(move || -> Result<Seed, String> {
        // Caminho rustls custom (com CRL) é ativado dentro de
        // `build_mtls_client` quando `server_crls_pem` é não-vazio.
        let client = build_mtls_client(&mat).map_err(|e| e.to_string())?;
        // Resolve "localhost" para o endereço local (porta aleatória).
        // `build_mtls_client` não expõe resolve; reconstruímos com
        // use_preconfigured_tls + resolve.
        let _ = client; // sanity de que a config compila.

        // Reconstrução com `.resolve()` — o caminho `build_mtls_client` é
        // testado pelo `..._succeeds` test; aqui replicamos a config rustls
        // diretamente para poder injetar resolve.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let server_ca = mcpix_bank_receiver::mtls::load_cert_chain(&mat.server_ca_pem)
            .map_err(|e| e.to_string())?;
        let crls = mcpix_bank_receiver::mtls::load_crls(&mat.server_crls_pem)
            .map_err(|e| e.to_string())?;
        let cli_chain = mcpix_bank_receiver::mtls::load_cert_chain(&mat.client_cert_pem)
            .map_err(|e| e.to_string())?;
        let cli_key = mcpix_bank_receiver::mtls::load_private_key(&mat.client_key_pem)
            .map_err(|e| e.to_string())?;
        let mut roots = rustls::RootCertStore::empty();
        for c in server_ca {
            roots.add(c).map_err(|e| e.to_string())?;
        }
        let verifier = rustls::client::WebPkiServerVerifier::builder(std::sync::Arc::new(roots))
            .with_crls(crls)
            .build()
            .map_err(|e| e.to_string())?;
        let tls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_client_auth_cert(cli_chain, cli_key)
            .map_err(|e| e.to_string())?;
        let resolved_client = reqwest::blocking::ClientBuilder::new()
            .use_preconfigured_tls(tls_config)
            .resolve("localhost", resolved)
            .build()
            .map_err(|e| e.to_string())?;
        let http_bank = HttpBankReceiver::with_client(base, resolved_client);
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

    assert!(
        result.is_err(),
        "expected client to reject revoked server cert, got Ok"
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
