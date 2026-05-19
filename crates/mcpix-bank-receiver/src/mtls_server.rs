//! Servidor mTLS — TLS termination com verificação obrigatória de client cert.
//!
//! ## Modelo
//!
//! O `ServerConfig` configura rustls com:
//! - cert/key do servidor (apresentados ao cliente)
//! - root store dos CAs aceitos para validar **certificados do cliente**
//! - `WebPkiClientVerifier` em modo obrigatório → handshake falha se o
//!   cliente não apresenta cert válido ou apresenta cert de CA não confiada
//!
//! ## O que mTLS garante aqui
//!
//! - **Confidencialidade + integridade** do canal (TLS 1.3 padrão)
//! - **Autenticação mútua**: ambos lados provam posse de chave privada
//!   correspondente a cert emitido pela CA da federação
//! - **Não-conexão de não-membros**: cliente sem cert válido recebe
//!   `CertificateRequired` no TLS layer, antes de qualquer requisição HTTP
//!
//! ## O que **não** garante (separado)
//!
//! - Mapping cert → `institution_id` ainda usa o header `X-Institution-Id`
//!   (ou app extrai via `mtls::extract_institution_id` chamado manualmente
//!   sobre o cert recuperado de outras formas). Em produção, o termination
//!   layer (envoy/nginx) propaga o cert via header `X-Forwarded-Client-Cert`
//!   parseado por middleware — abordagem padrão e auditável.
//! - Live OCSP query — apenas stapling. Operador atualiza a OCSP response
//!   carimbada periodicamente (cron + reload).

use std::net::SocketAddr;
use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};

use mcpix_core::error::McpixError;

use crate::http_server::router;
use crate::mtls::{load_cert_chain, load_crls, load_private_key};
use crate::BankReceiver;

/// Configuração completa de TLS do servidor. Use o construtor
/// [`ServerTlsConfig::new`] para os campos obrigatórios e os setters
/// para revogação opcional.
#[derive(Clone, Debug)]
pub struct ServerTlsConfig<'a> {
    /// Cert chain do servidor (apresentado ao cliente).
    pub server_cert_pem: &'a [u8],
    /// Chave privada do servidor (PKCS#8, RSA ou SEC1).
    pub server_key_pem: &'a [u8],
    /// CA que assinou os client certs aceitos.
    pub client_ca_pem: &'a [u8],
    /// CRLs PEM concatenadas usadas para revogar **client certs**.
    /// Vazio = sem revogação (não recomendado em produção).
    pub client_crls_pem: &'a [u8],
    /// DER da OCSP response para stapling ao próprio cert do servidor.
    /// Vazio = sem stapling. Operador deve atualizar periodicamente.
    pub ocsp_response: &'a [u8],
}

impl<'a> ServerTlsConfig<'a> {
    pub fn new(
        server_cert_pem: &'a [u8],
        server_key_pem: &'a [u8],
        client_ca_pem: &'a [u8],
    ) -> Self {
        Self {
            server_cert_pem,
            server_key_pem,
            client_ca_pem,
            client_crls_pem: &[],
            ocsp_response: &[],
        }
    }

    pub fn with_client_crls(mut self, crls_pem: &'a [u8]) -> Self {
        self.client_crls_pem = crls_pem;
        self
    }

    pub fn with_stapled_ocsp(mut self, ocsp_der: &'a [u8]) -> Self {
        self.ocsp_response = ocsp_der;
        self
    }
}

/// Constrói o `ServerConfig` rustls para mTLS com revogação opcional.
///
/// Se `cfg.client_crls_pem` for não-vazio, o verifier valida cada client
/// cert apresentado contra as CRLs — cert revogado falha o handshake
/// com `unknown_ca` no log do rustls.
///
/// Se `cfg.ocsp_response` for não-vazio, o servidor faz stapling
/// (TLS Certificate Status Extension) anexando a resposta OCSP da CA
/// para o próprio cert. Cliente que valida stapling (rustls default
/// para clients com `WebPkiServerVerifier`) detecta revogação.
pub fn build_server_config_full(cfg: &ServerTlsConfig<'_>) -> Result<ServerConfig, McpixError> {
    // `ring` é o provider crypto default; instalá-lo uma vez é idempotente.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let server_chain = load_cert_chain(cfg.server_cert_pem)?;
    let server_key = load_private_key(cfg.server_key_pem)?;
    let client_ca_chain = load_cert_chain(cfg.client_ca_pem)?;
    let client_crls = load_crls(cfg.client_crls_pem)?;

    let mut roots = RootCertStore::empty();
    for c in client_ca_chain {
        roots
            .add(c)
            .map_err(|e| McpixError::Transport(format!("add CA: {e}")))?;
    }

    let verifier_builder = WebPkiClientVerifier::builder(Arc::new(roots));
    let verifier = if client_crls.is_empty() {
        verifier_builder.build()
    } else {
        verifier_builder.with_crls(client_crls).build()
    }
    .map_err(|e| McpixError::Transport(format!("client verifier: {e}")))?;

    let builder = ServerConfig::builder().with_client_cert_verifier(verifier);

    if cfg.ocsp_response.is_empty() {
        builder
            .with_single_cert(server_chain, server_key)
            .map_err(|e| McpixError::Transport(format!("server cert: {e}")))
    } else {
        builder
            .with_single_cert_with_ocsp(server_chain, server_key, cfg.ocsp_response.to_vec())
            .map_err(|e| McpixError::Transport(format!("server cert + ocsp: {e}")))
    }
}

/// Wrapper compat: equivalente a `build_server_config_full` com CRL/OCSP vazios.
pub fn build_server_config(
    server_cert_pem: &[u8],
    server_key_pem: &[u8],
    client_ca_pem: &[u8],
) -> Result<ServerConfig, McpixError> {
    build_server_config_full(&ServerTlsConfig::new(
        server_cert_pem,
        server_key_pem,
        client_ca_pem,
    ))
}

pub type ServerHandle = axum_server::Handle<SocketAddr>;

/// Sobe o servidor mTLS em `addr`. Retorna `SocketAddr` resolvida + `Handle`
/// para encerramento gracioso (`handle.shutdown()` em qualquer thread).
pub async fn serve_mtls(
    addr: SocketAddr,
    bank: Arc<dyn BankReceiver>,
    config: ServerConfig,
) -> Result<(SocketAddr, ServerHandle), std::io::Error> {
    // axum-server 0.8 não expõe API direta de "bind 0 + retornar SocketAddr"
    // junto com TLS. Bindamos um std TcpListener primeiro, descobrimos a
    // porta, e passamos para o axum-server.
    let std_listener = std::net::TcpListener::bind(addr)?;
    let bound = std_listener.local_addr()?;
    std_listener.set_nonblocking(true)?;

    let handle: ServerHandle = axum_server::Handle::new();
    let handle_clone = handle.clone();
    let app = router(bank);
    let rustls_cfg = RustlsConfig::from_config(Arc::new(config));

    let server = axum_server::from_tcp_rustls(std_listener, rustls_cfg)?.handle(handle_clone);
    tokio::spawn(async move {
        let _ = server.serve(app.into_make_service()).await;
    });

    // Aguarda o servidor ficar pronto — caso contrário a primeira conexão
    // do teste pode bater antes do listener começar a aceitar.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok((bound, handle))
}
