//! Cliente HTTP do banco recebedor — implementa o trait `BankReceiver`
//! contra o servidor REST de `http_server`.
//!
//! Uso típico: o banco do pagador instancia `HttpBankReceiver::new(base_url)`
//! e injeta no `PayerBankMock::new(&client)`. Toda chamada `lookup_seed` vira
//! GET HTTP transparentemente.
//!
//! Bloqueante (reqwest blocking) para manter o trait `BankReceiver` sync —
//! o trait do núcleo não tem `async fn`. Em runtime async, o caller envelopa
//! com `tokio::task::spawn_blocking`.

use mcpix_core::error::McpixError;
use mcpix_core::types::{Seed, SeedId};
use mcpix_core::version::ProtocolVersion;

use crate::wire::{CapabilitiesPayload, SeedPayload};
use crate::{BankReceiver, Requester};

pub struct HttpBankReceiver {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl HttpBankReceiver {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with_client(base_url, reqwest::blocking::Client::new())
    }

    pub fn with_client(base_url: impl Into<String>, client: reqwest::blocking::Client) -> Self {
        let mut url = base_url.into();
        // Normaliza removendo trailing slash para que `format!("{base}/v1/...")`
        // não produza "//".
        while url.ends_with('/') {
            url.pop();
        }
        Self { base_url: url, client }
    }

    fn seed_url(&self, seed_id: &SeedId) -> String {
        format!("{}/v1/seeds/{}", self.base_url, seed_id.as_str())
    }

    fn capabilities_url(&self) -> String {
        format!("{}/v1/capabilities", self.base_url)
    }
}

impl BankReceiver for HttpBankReceiver {
    fn register_seed(&self, seed_id: &SeedId, seed: Seed) -> Result<(), McpixError> {
        let body = SeedPayload::from_seed(&seed);
        let resp = self
            .client
            .post(self.seed_url(seed_id))
            .json(&body)
            .send()
            .map_err(|e| McpixError::Transport(format!("POST seed: {e}")))?;
        if !resp.status().is_success() {
            return Err(McpixError::Transport(format!(
                "POST seed returned {}",
                resp.status()
            )));
        }
        Ok(())
    }

    fn lookup_seed(
        &self,
        seed_id: &SeedId,
        requester: &Requester,
    ) -> Result<Seed, McpixError> {
        let resp = self
            .client
            .get(self.seed_url(seed_id))
            .header("x-institution-id", &requester.institution_id)
            .send()
            .map_err(|e| McpixError::Transport(format!("GET seed: {e}")))?;
        match resp.status() {
            s if s.is_success() => {
                let payload: SeedPayload = resp
                    .json()
                    .map_err(|e| McpixError::Transport(format!("decode seed: {e}")))?;
                payload.into_seed()
            }
            s if s == reqwest::StatusCode::NOT_FOUND => Err(McpixError::UnknownSeed),
            s => Err(McpixError::Transport(format!("GET seed returned {s}"))),
        }
    }

    /// Sobrescrita do default da trait: consulta o peer remoto via
    /// `GET /v1/capabilities`. Strings desconhecidas pelo enum local
    /// são ignoradas — não falha se o peer reportar `PIXOFFv9`.
    fn supported_versions(&self) -> Result<Vec<ProtocolVersion>, McpixError> {
        let resp = self
            .client
            .get(self.capabilities_url())
            .send()
            .map_err(|e| McpixError::Transport(format!("GET capabilities: {e}")))?;
        if !resp.status().is_success() {
            return Err(McpixError::Transport(format!(
                "GET capabilities returned {}",
                resp.status()
            )));
        }
        let payload: CapabilitiesPayload = resp
            .json()
            .map_err(|e| McpixError::Transport(format!("decode capabilities: {e}")))?;
        // Filtra para variantes conhecidas deste build. Versões
        // anunciadas que ainda não estão no nosso enum somem aqui —
        // o helper `version::negotiate_version` (que opera sobre as
        // strings cruas) preserva a info quando o caller precisa.
        let mut out = Vec::with_capacity(payload.versions.len());
        for v in ProtocolVersion::all() {
            if payload.versions.iter().any(|s| s == v.prefix()) {
                out.push(*v);
            }
        }
        Ok(out)
    }
}
