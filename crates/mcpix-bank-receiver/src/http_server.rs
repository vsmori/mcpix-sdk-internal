//! Servidor REST do banco recebedor (axum).
//!
//! Endpoints (todos sob `/v1`):
//!   POST   /v1/seeds/{seed_id}      registra semente (body: SeedPayload)
//!   GET    /v1/seeds/{seed_id}      retorna semente (body: SeedPayload)
//!   GET    /v1/capabilities         versões do protocolo suportadas
//!   GET    /v1/healthz              liveness probe
//!
//! Auth: **nenhuma** nesta sessão. O contrato `BankReceiver::lookup_seed`
//! recebe `Requester { institution_id }`; aqui usamos um header
//! `X-Institution-Id` como placeholder. Em produção, mTLS + verificação
//! de fingerprint do client cert no termination layer (nginx, envoy) ou
//! mTLS direto no axum. Linha onde encaixa: o handler já tem `requester`
//! pronto para receber o valor extraído do cert.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tokio::net::TcpListener;

use crate::wire::{CapabilitiesPayload, ErrorBody, SeedPayload};
use crate::{BankReceiver, Requester};
use mcpix_core::error::McpixError;
use mcpix_core::types::SeedId;

/// Constrói o `Router` axum a partir de uma impl `BankReceiver`. Útil para
/// embedar em testes (`axum::serve(listener, router)`).
pub fn router(bank: Arc<dyn BankReceiver>) -> Router {
    Router::new()
        .route("/v1/seeds/{seed_id}", post(put_seed).get(get_seed))
        .route("/v1/capabilities", get(get_capabilities))
        .route("/v1/healthz", get(|| async { "ok" }))
        .with_state(bank)
}

/// Binda um listener em `addr` e serve até o future `shutdown` resolver.
/// Retorna a `SocketAddr` real (útil quando addr = "127.0.0.1:0").
pub async fn serve(
    addr: SocketAddr,
    bank: Arc<dyn BankReceiver>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<SocketAddr, std::io::Error> {
    let listener = TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    let app = router(bank);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await;
    });
    Ok(bound)
}

// ─────────────────────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────────────────────

async fn put_seed(
    State(bank): State<Arc<dyn BankReceiver>>,
    Path(seed_id): Path<String>,
    Json(payload): Json<SeedPayload>,
) -> Response {
    let sid = match SeedId::new(seed_id) {
        Ok(s) => s,
        Err(e) => return mcpix_err(StatusCode::BAD_REQUEST, e),
    };
    let seed = match payload.into_seed() {
        Ok(s) => s,
        Err(e) => return mcpix_err(StatusCode::BAD_REQUEST, e),
    };
    match bank.register_seed(&sid, seed) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => mcpix_err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn get_seed(
    State(bank): State<Arc<dyn BankReceiver>>,
    Path(seed_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let sid = match SeedId::new(seed_id) {
        Ok(s) => s,
        Err(e) => return mcpix_err(StatusCode::BAD_REQUEST, e),
    };
    // Placeholder de autenticação — preenche `Requester` com header.
    let institution_id = headers
        .get("x-institution-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous")
        .to_string();
    let requester = Requester { institution_id };

    match bank.lookup_seed(&sid, &requester) {
        Ok(seed) => (StatusCode::OK, Json(SeedPayload::from_seed(&seed))).into_response(),
        Err(McpixError::UnknownSeed) => mcpix_err(StatusCode::NOT_FOUND, McpixError::UnknownSeed),
        Err(e) => mcpix_err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn get_capabilities(State(bank): State<Arc<dyn BankReceiver>>) -> Response {
    match bank.supported_versions() {
        Ok(versions) => {
            let payload = CapabilitiesPayload {
                versions: versions.iter().map(|v| v.prefix().to_string()).collect(),
            };
            (StatusCode::OK, Json(payload)).into_response()
        }
        Err(e) => mcpix_err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

fn mcpix_err(status: StatusCode, err: McpixError) -> Response {
    let body = ErrorBody {
        error: err.to_string(),
    };
    (status, Json(json!(body))).into_response()
}

// Testes de integração que envolvem `http-client` vivem em
// `tests/http_e2e.rs` — exigem que ambas features estejam ligadas.
