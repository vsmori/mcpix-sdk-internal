//! OCSP — Online Certificate Status Protocol (RFC 6960).
//!
//! **Posicionamento.** S14 entregou CRL (lista pull-based, atualização
//! periódica) e OCSP **stapling** (response carimbada pelo servidor no
//! handshake). Este módulo adiciona a terceira variante: **live OCSP
//! query** — o cliente pergunta diretamente ao responder da CA, em
//! tempo de operação, se um cert específico foi revogado. Use quando
//! a CRL pode estar minutos atrás da decisão de revogação.
//!
//! ## Trade-off vs CRL/stapling
//!
//! | Aspecto | CRL | Stapling | **Live OCSP** |
//! |---|---|---|---|
//! | Frescor da info | minutos a horas | minutos | **segundos** |
//! | Latência por handshake | 0 (offline) | 0 | **+1 RTT** |
//! | Privacy | local | local | **CA sabe qual cert você consultou** |
//! | Disponibilidade | offline | offline | **depende do responder** |
//!
//! Use live OCSP **antes de operações sensíveis** (transações de alto
//! valor, key disclosure), não em todo handshake.
//!
//! ## Escopo desta Phase 1
//!
//! Provido neste módulo:
//! - [`build_ocsp_request`] — DER do `OCSPRequest`.
//! - [`send_ocsp_request`] — HTTP POST ao responder.
//! - [`parse_ocsp_response`] — extrai `OcspStatus` do DER de resposta.
//! - [`OcspChecker`] — alto nível, encadeia os três.
//!
//! **Out of scope nesta fase**: verificação da assinatura da
//! `OCSPResponse` contra a CA. O integrator faz isso usando o cert
//! chain que já tem da config mTLS — snippet em
//! `docs/MTLS_REVOCATION.md`. Sem verificação, a confiabilidade
//! depende do canal entre cliente e responder (HTTPS para o responder
//! ajuda mas não substitui assinatura da response).

use std::io::BufReader;

use sha1::Sha1;
use x509_cert::der::{Decode, Encode};
use x509_cert::Certificate;
use x509_ocsp::builder::OcspRequestBuilder;
use x509_ocsp::{
    CertId, CertStatus, OcspRequest, OcspResponse, OcspResponseStatus, Request, Version,
};

use mcpix_core::error::McpixError;

/// Status de revogação devolvido pelo responder OCSP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcspStatus {
    /// Cert válido conforme o responder.
    Good,
    /// Cert revogado. `reason_code` pode ser `None` se o responder
    /// não populou (campo opcional na RFC 6960 §4.2.1).
    Revoked { reason_code: Option<u32> },
    /// Responder não conhece este cert. **Comportamento na sua
    /// aplicação é política** — alguns sistemas tratam como
    /// revogação (fail-closed), outros como erro transitório.
    Unknown,
}

/// Constrói o DER de um `OCSPRequest` perguntando pelo status do
/// `subject_cert` emitido pelo `issuer_cert`.
///
/// Ambos os PEM devem conter exatamente uma cadeia de cert; o
/// primeiro cert é usado. Os hashes do issuer são feitos com SHA-1
/// (padrão RFC 6960; a maioria dos responders só suporta SHA-1).
pub fn build_ocsp_request(
    subject_cert_pem: &[u8],
    issuer_cert_pem: &[u8],
) -> Result<Vec<u8>, McpixError> {
    let subject = first_cert(subject_cert_pem, "subject")?;
    let issuer = first_cert(issuer_cert_pem, "issuer")?;

    let cert_id = CertId::from_cert::<Sha1>(&issuer, &subject)
        .map_err(|e| McpixError::Transport(format!("ocsp: build CertId: {e}")))?;

    let request = Request {
        req_cert: cert_id,
        single_request_extensions: None,
    };
    let req: OcspRequest = OcspRequestBuilder::new(Version::V1)
        .with_request(request)
        .build();

    req.to_der()
        .map_err(|e| McpixError::Transport(format!("ocsp: encode request: {e}")))
}

/// Faz POST do `request_der` para `responder_url` com `Content-Type:
/// application/ocsp-request`. Retorna o DER cru da response.
///
/// Erros: rede, status HTTP non-2xx, response vazia.
pub fn send_ocsp_request(
    client: &reqwest::blocking::Client,
    responder_url: &str,
    request_der: &[u8],
) -> Result<Vec<u8>, McpixError> {
    let resp = client
        .post(responder_url)
        .header("Content-Type", "application/ocsp-request")
        .header("Accept", "application/ocsp-response")
        .body(request_der.to_vec())
        .send()
        .map_err(|e| McpixError::Transport(format!("ocsp: POST: {e}")))?;
    if !resp.status().is_success() {
        return Err(McpixError::Transport(format!(
            "ocsp: responder returned {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .map_err(|e| McpixError::Transport(format!("ocsp: read response body: {e}")))?
        .to_vec();
    if bytes.is_empty() {
        return Err(McpixError::Transport("ocsp: empty response".into()));
    }
    Ok(bytes)
}

/// Parseia o DER de um `OCSPResponse` e extrai o status do **primeiro**
/// `SingleResponse` (na prática só há um, porque mandamos um único
/// `Request` no `OCSPRequest`).
///
/// **NÃO verifica assinatura** — vide nota no nível de módulo.
pub fn parse_ocsp_response(response_der: &[u8]) -> Result<OcspStatus, McpixError> {
    let response = OcspResponse::from_der(response_der)
        .map_err(|e| McpixError::Transport(format!("ocsp: decode response: {e}")))?;

    if response.response_status != OcspResponseStatus::Successful {
        return Err(McpixError::Transport(format!(
            "ocsp: non-successful response status: {:?}",
            response.response_status
        )));
    }

    let response_bytes = response.response_bytes.ok_or_else(|| {
        McpixError::Transport("ocsp: successful response without responseBytes".into())
    })?;

    // RFC 6960 §4.2.1: o único tipo registrado de responseBytes em
    // resposta successful é `id-pkix-ocsp-basic` carregando um
    // BasicOcspResponse.
    let basic: x509_ocsp::BasicOcspResponse =
        x509_cert::der::Decode::from_der(response_bytes.response.as_bytes())
            .map_err(|e| McpixError::Transport(format!("ocsp: decode BasicOcspResponse: {e}")))?;

    let single = basic
        .tbs_response_data
        .responses
        .first()
        .ok_or_else(|| McpixError::Transport("ocsp: empty SingleResponse list".into()))?;

    Ok(match &single.cert_status {
        CertStatus::Good(_) => OcspStatus::Good,
        CertStatus::Revoked(info) => OcspStatus::Revoked {
            reason_code: info.revocation_reason.map(|r| r as u32),
        },
        CertStatus::Unknown(_) => OcspStatus::Unknown,
    })
}

/// Alto nível: encadeia build + send + parse. Útil quando você só
/// quer "o status deste cert agora".
///
/// **Não** integra com rustls — é uma consulta out-of-band. Use
/// antes de iniciar operações sensíveis sobre a conexão mTLS:
///
/// ```ignore
/// let checker = OcspChecker::new(&client, "http://ocsp.bank-ca.example/");
/// let status = checker.check(&server_cert_pem, &issuer_cert_pem)?;
/// if !matches!(status, OcspStatus::Good) {
///     return Err(/* abort transaction */);
/// }
/// ```
pub struct OcspChecker<'a> {
    client: &'a reqwest::blocking::Client,
    responder_url: String,
}

impl<'a> OcspChecker<'a> {
    pub fn new(client: &'a reqwest::blocking::Client, responder_url: impl Into<String>) -> Self {
        Self {
            client,
            responder_url: responder_url.into(),
        }
    }

    pub fn check(
        &self,
        subject_cert_pem: &[u8],
        issuer_cert_pem: &[u8],
    ) -> Result<OcspStatus, McpixError> {
        let req = build_ocsp_request(subject_cert_pem, issuer_cert_pem)?;
        let resp = send_ocsp_request(self.client, &self.responder_url, &req)?;
        parse_ocsp_response(&resp)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn first_cert(pem: &[u8], role: &str) -> Result<Certificate, McpixError> {
    let mut rd = BufReader::new(pem);
    let der = rustls_pemfile::certs(&mut rd)
        .next()
        .ok_or_else(|| McpixError::Transport(format!("ocsp: no cert in {role} pem")))?
        .map_err(|e| McpixError::Transport(format!("ocsp: read {role} pem: {e}")))?;
    Certificate::from_der(&der)
        .map_err(|e| McpixError::Transport(format!("ocsp: parse {role} DER: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pequena PKI de teste: CA self-signed + subject cert.
    fn test_pki() -> (Vec<u8>, Vec<u8>) {
        use rcgen::{CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};

        let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let mut ca_dn = rcgen::DistinguishedName::new();
        ca_dn.push(DnType::CommonName, "ocsp-test-ca");
        ca_params.distinguished_name = ca_dn;
        let ca_kp = KeyPair::generate().unwrap();
        let ca_cert = ca_params.self_signed(&ca_kp).unwrap();
        let ca_pem = ca_cert.pem().into_bytes();

        let issuer = Issuer::from_params(&ca_params, &ca_kp);

        let mut subj_params = CertificateParams::new(vec!["test-subject".to_string()]).unwrap();
        let mut subj_dn = rcgen::DistinguishedName::new();
        subj_dn.push(DnType::CommonName, "subject");
        subj_params.distinguished_name = subj_dn;
        let subj_kp = KeyPair::generate().unwrap();
        let subj_cert = subj_params.signed_by(&subj_kp, &issuer).unwrap();
        let subj_pem = subj_cert.pem().into_bytes();

        (subj_pem, ca_pem)
    }

    #[test]
    fn build_request_produces_well_formed_der() {
        let (subj, ca) = test_pki();
        let der = build_ocsp_request(&subj, &ca).expect("build OCSP request");

        // DER round-trip: deve ser decodificável como OcspRequest.
        let decoded = OcspRequest::from_der(&der).expect("OCSP request DER round-trips");
        assert_eq!(decoded.tbs_request.version, Version::V1);
        assert_eq!(decoded.tbs_request.request_list.len(), 1);
    }

    #[test]
    fn build_request_fails_on_garbage_pem() {
        let (_, ca) = test_pki();
        let err = build_ocsp_request(b"---NOT A CERT---", &ca).unwrap_err();
        match err {
            McpixError::Transport(msg) => {
                assert!(
                    msg.contains("no cert in subject pem") || msg.contains("ocsp"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected Transport error, got {other:?}"),
        }
    }

    #[test]
    fn parse_response_rejects_garbage() {
        let err = parse_ocsp_response(b"\x01garbage").unwrap_err();
        assert!(matches!(err, McpixError::Transport(_)));
    }

    #[test]
    fn parse_response_rejects_empty() {
        let err = parse_ocsp_response(b"").unwrap_err();
        assert!(matches!(err, McpixError::Transport(_)));
    }

    // Sinaliza ao integrator que o status enum cobre todos os
    // variants RFC 6960. Se a x509-ocsp introduzir um CertStatus
    // novo, o exhaustive match no parser quebrará a build aqui.
    #[test]
    fn status_enum_is_exhaustive() {
        let _ = OcspStatus::Good;
        let _ = OcspStatus::Revoked {
            reason_code: Some(1),
        };
        let _ = OcspStatus::Revoked { reason_code: None };
        let _ = OcspStatus::Unknown;
    }
}
