//! Cliente mTLS — wrapper sobre `HttpBankReceiver` que monta o
//! `reqwest::blocking::Client` com client cert + CA confiada.
//!
//! Distinção em relação ao `HttpBankReceiver` plain:
//! - Aceita conexão apenas a servidores que apresentem cert assinado pela CA
//!   configurada (sem fallback para system roots — federação fechada).
//! - Apresenta o próprio cert no handshake; servidor que não exigir client cert
//!   ignora; servidor mTLS valida contra sua CA.

use std::sync::Arc;

use rustls::client::WebPkiServerVerifier;
use rustls::{ClientConfig, RootCertStore};

use mcpix_core::error::McpixError;

use crate::http_client::HttpBankReceiver;
use crate::mtls::{load_cert_chain, load_crls, load_private_key};

/// Material de cliente mTLS. Aceita o cert chain e a chave privada PEM,
/// junto com a CA do servidor. Campo opcional `server_crls_pem` ativa
/// verificação de revogação contra o cert do servidor.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct MtlsClientMaterial {
    /// PEM concatenado: client cert + intermediários (se houver).
    pub client_cert_pem: Vec<u8>,
    /// PEM da chave privada do cliente (PKCS#8, RSA ou SEC1).
    pub client_key_pem: Vec<u8>,
    /// PEM da CA que assinou o cert do servidor.
    pub server_ca_pem: Vec<u8>,
    /// CRLs PEM concatenadas usadas para revogar **server certs**.
    /// Vazio (default) = sem verificação CRL local, usa caminho `reqwest`
    /// padrão. Não-vazio = constrói `rustls::ClientConfig` custom com
    /// `WebPkiServerVerifier` configurado com as CRLs.
    pub server_crls_pem: Vec<u8>,
}

impl MtlsClientMaterial {
    pub fn new(
        client_cert_pem: impl Into<Vec<u8>>,
        client_key_pem: impl Into<Vec<u8>>,
        server_ca_pem: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            client_cert_pem: client_cert_pem.into(),
            client_key_pem: client_key_pem.into(),
            server_ca_pem: server_ca_pem.into(),
            server_crls_pem: Vec::new(),
        }
    }

    pub fn with_server_crls(mut self, crls_pem: impl Into<Vec<u8>>) -> Self {
        self.server_crls_pem = crls_pem.into();
        self
    }
}

pub fn build_mtls_client(
    material: &MtlsClientMaterial,
) -> Result<reqwest::blocking::Client, McpixError> {
    if material.server_crls_pem.is_empty() {
        build_mtls_client_legacy(material)
    } else {
        build_mtls_client_with_crls(material)
    }
}

// ──── Caminho legado (sem CRLs): API alto-nível do reqwest ─────────────
fn build_mtls_client_legacy(
    material: &MtlsClientMaterial,
) -> Result<reqwest::blocking::Client, McpixError> {
    let mut id_pem =
        Vec::with_capacity(material.client_cert_pem.len() + material.client_key_pem.len() + 1);
    id_pem.extend_from_slice(&material.client_cert_pem);
    if !id_pem.ends_with(b"\n") {
        id_pem.push(b'\n');
    }
    id_pem.extend_from_slice(&material.client_key_pem);

    let identity = reqwest::Identity::from_pem(&id_pem)
        .map_err(|e| McpixError::Transport(format!("identity from pem: {e}")))?;

    let ca_cert = reqwest::Certificate::from_pem(&material.server_ca_pem)
        .map_err(|e| McpixError::Transport(format!("ca cert: {e}")))?;

    reqwest::blocking::ClientBuilder::new()
        .use_rustls_tls()
        .add_root_certificate(ca_cert)
        .identity(identity)
        .build()
        .map_err(|e| McpixError::Transport(format!("build mtls client: {e}")))
}

// ──── Caminho com CRL: ClientConfig rustls custom ──────────────────────
//
// O `WebPkiServerVerifier` é configurado com root store + CRLs do operador.
// `reqwest::use_preconfigured_tls` injeta a config inteira (incluindo o
// `with_client_auth_cert` para o lado mTLS). Os system roots NÃO são
// adicionados — federação fechada exige confiança explícita na CA.
fn build_mtls_client_with_crls(
    material: &MtlsClientMaterial,
) -> Result<reqwest::blocking::Client, McpixError> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let server_ca_chain = load_cert_chain(&material.server_ca_pem)?;
    let client_chain = load_cert_chain(&material.client_cert_pem)?;
    let client_key = load_private_key(&material.client_key_pem)?;
    let crls = load_crls(&material.server_crls_pem)?;
    debug_assert!(!crls.is_empty()); // garantido pelo dispatch acima.

    let mut roots = RootCertStore::empty();
    for c in server_ca_chain {
        roots
            .add(c)
            .map_err(|e| McpixError::Transport(format!("add server CA: {e}")))?;
    }

    let server_verifier = WebPkiServerVerifier::builder(Arc::new(roots))
        .with_crls(crls)
        .build()
        .map_err(|e| McpixError::Transport(format!("server verifier: {e}")))?;

    let tls_config = ClientConfig::builder()
        .dangerous() // o "dangerous" aqui é o gateway para verifier custom;
        // estamos passando o WebPkiServerVerifier oficial — não é uma
        // bypass de validação, apenas a forma de aceitar uma instância
        // pré-construída com CRLs anexadas.
        .with_custom_certificate_verifier(server_verifier)
        .with_client_auth_cert(client_chain, client_key)
        .map_err(|e| McpixError::Transport(format!("client auth cert: {e}")))?;

    reqwest::blocking::ClientBuilder::new()
        .use_preconfigured_tls(tls_config)
        .build()
        .map_err(|e| McpixError::Transport(format!("build mtls client w/ crl: {e}")))
}

/// Conveniência: monta `HttpBankReceiver` com cliente mTLS pré-configurado.
pub fn http_bank_receiver_mtls(
    base_url: impl Into<String>,
    material: &MtlsClientMaterial,
) -> Result<HttpBankReceiver, McpixError> {
    let client = build_mtls_client(material)?;
    Ok(HttpBankReceiver::with_client(base_url, client))
}
