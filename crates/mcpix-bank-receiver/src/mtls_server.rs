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
//! - Revogação (OCSP/CRL) — fica para sessão de PKI completa.

use std::net::SocketAddr;
use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};

use mcpix_core::error::McpixError;

use crate::http_server::router;
use crate::mtls::{load_cert_chain, load_private_key};
use crate::BankReceiver;

/// Constrói o `ServerConfig` rustls para mTLS:
/// - apresenta `server_cert` + `server_key`
/// - aceita apenas clientes com cert verificado contra `client_ca`
pub fn build_server_config(
    server_cert_pem: &[u8],
    server_key_pem: &[u8],
    client_ca_pem: &[u8],
) -> Result<ServerConfig, McpixError> {
    // `ring` é o provider crypto default; instalá-lo uma vez é idempotente
    // (`install_default` retorna Err em subsequent calls — ignoramos).
    let _ = rustls::crypto::ring::default_provider().install_default();

    let server_chain = load_cert_chain(server_cert_pem)?;
    let server_key = load_private_key(server_key_pem)?;
    let client_ca_chain = load_cert_chain(client_ca_pem)?;

    let mut roots = RootCertStore::empty();
    for c in client_ca_chain {
        roots
            .add(c)
            .map_err(|e| McpixError::Transport(format!("add CA: {e}")))?;
    }
    let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .map_err(|e| McpixError::Transport(format!("client verifier: {e}")))?;

    ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(server_chain, server_key)
        .map_err(|e| McpixError::Transport(format!("server cert: {e}")))
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

    let server = axum_server::from_tcp_rustls(std_listener, rustls_cfg)?
        .handle(handle_clone);
    tokio::spawn(async move {
        let _ = server.serve(app.into_make_service()).await;
    });

    // Aguarda o servidor ficar pronto — caso contrário a primeira conexão
    // do teste pode bater antes do listener começar a aceitar.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok((bound, handle))
}
