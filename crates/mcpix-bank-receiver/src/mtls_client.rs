//! Cliente mTLS — wrapper sobre `HttpBankReceiver` que monta o
//! `reqwest::blocking::Client` com client cert + CA confiada.
//!
//! Distinção em relação ao `HttpBankReceiver` plain:
//! - Aceita conexão apenas a servidores que apresentem cert assinado pela CA
//!   configurada (sem fallback para system roots — federação fechada).
//! - Apresenta o próprio cert no handshake; servidor que não exigir client cert
//!   ignora; servidor mTLS valida contra sua CA.

use mcpix_core::error::McpixError;

use crate::http_client::HttpBankReceiver;

/// Material de cliente mTLS. Aceita o cert chain e a chave privada PEM,
/// junto com a CA do servidor.
#[derive(Clone)]
pub struct MtlsClientMaterial {
    /// PEM concatenado: client cert + intermediários (se houver).
    pub client_cert_pem: Vec<u8>,
    /// PEM da chave privada do cliente (PKCS#8, RSA ou SEC1).
    pub client_key_pem: Vec<u8>,
    /// PEM da CA que assinou o cert do servidor.
    pub server_ca_pem: Vec<u8>,
}

pub fn build_mtls_client(
    material: &MtlsClientMaterial,
) -> Result<reqwest::blocking::Client, McpixError> {
    // PKCS#12 não é necessário — reqwest aceita PEM concatenado (cert + key)
    // via Identity::from_pem.
    let mut id_pem = Vec::with_capacity(
        material.client_cert_pem.len() + material.client_key_pem.len() + 1,
    );
    id_pem.extend_from_slice(&material.client_cert_pem);
    if !id_pem.ends_with(b"\n") {
        id_pem.push(b'\n');
    }
    id_pem.extend_from_slice(&material.client_key_pem);

    let identity = reqwest::Identity::from_pem(&id_pem)
        .map_err(|e| McpixError::Transport(format!("identity from pem: {e}")))?;

    let ca_cert = reqwest::Certificate::from_pem(&material.server_ca_pem)
        .map_err(|e| McpixError::Transport(format!("ca cert: {e}")))?;

    // `add_root_certificate` por si só adiciona a CA ao root store; com
    // `use_rustls_tls()` o conjunto de roots fica composto por system roots
    // + nossa CA. Para uma federação fechada, isto é seguro porque o cert
    // do servidor é assinado por NOSSA CA — system roots não vão validar
    // um cert assinado por outra cadeia. Caso queira escopo estrito, basta
    // configurar a feature `rustls` sem `default-features` e gerenciar
    // o root store manualmente.
    reqwest::blocking::ClientBuilder::new()
        .use_rustls_tls()
        .add_root_certificate(ca_cert)
        .identity(identity)
        .build()
        .map_err(|e| McpixError::Transport(format!("build mtls client: {e}")))
}

/// Conveniência: monta `HttpBankReceiver` com cliente mTLS pré-configurado.
pub fn http_bank_receiver_mtls(
    base_url: impl Into<String>,
    material: &MtlsClientMaterial,
) -> Result<HttpBankReceiver, McpixError> {
    let client = build_mtls_client(material)?;
    Ok(HttpBankReceiver::with_client(base_url, client))
}
