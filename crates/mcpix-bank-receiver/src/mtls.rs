//! Suporte a mTLS (Mutual TLS) para o canal banco-pagador → banco-recebedor.
//!
//! ## Modelo de confiança
//!
//! - Uma **CA interna** assina certificados para cada instituição participante.
//! - O **servidor** apresenta certificado assinado pela CA e exige que o
//!   cliente também apresente certificado assinado pela mesma CA.
//! - O servidor extrai o **SAN URI** (ou CN como fallback) do client cert
//!   e popula `Requester::institution_id` com o valor — substituindo o
//!   header `X-Institution-Id` placeholder da S7.
//!
//! ## Por que SAN URI e não CN
//!
//! CN é deprecated em validação de identidade desde RFC 6125. SAN URI no
//! formato `urn:mcpix:institution:<id>` é estruturado, sem ambiguidade
//! de hostname vs identity. Fallback para CN existe só para PKIs legadas.
//!
//! ## Limites
//!
//! - Não fazemos revogação (OCSP/CRL) nesta sessão — a CA confiada é única
//!   e a rotação se dá por re-emissão.
//! - mTLS protege o canal e identifica peers. **Não** prova que o servidor
//!   é a impl `BankReceiver` correta — isso continua sendo responsabilidade
//!   da configuração da CA e do deploy.

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::io::BufReader;

use mcpix_core::error::McpixError;

pub const INSTITUTION_URI_PREFIX: &str = "urn:mcpix:institution:";

/// Material PEM (cert + key) carregado de bytes — útil quando os certificados
/// vêm de um keystore ou de bytes embutidos.
#[derive(Clone, Debug)]
pub struct PemMaterial<'a> {
    pub cert_chain_pem: &'a [u8],
    pub private_key_pem: &'a [u8],
}

/// Lê um cert chain PEM e devolve `Vec<CertificateDer>` para uso direto com rustls.
pub fn load_cert_chain(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, McpixError> {
    let mut rd = BufReader::new(pem);
    rustls_pemfile::certs(&mut rd)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| McpixError::Transport(format!("read cert pem: {e}")))
}

/// Lê uma private key PEM (PKCS#8 ou RSA) e devolve `PrivateKeyDer`.
pub fn load_private_key(pem: &[u8]) -> Result<PrivateKeyDer<'static>, McpixError> {
    let mut rd = BufReader::new(pem);
    // `private_key` aceita PKCS#8, RSA e SEC1 — cobre o que `rcgen` emite.
    rustls_pemfile::private_key(&mut rd)
        .map_err(|e| McpixError::Transport(format!("read key pem: {e}")))?
        .ok_or_else(|| McpixError::Transport("no private key found in pem".into()))
}

/// Extrai a identidade da instituição do certificado do cliente.
/// Preferência: SAN URI prefixado por `urn:mcpix:institution:`. Fallback: CN.
pub fn extract_institution_id(cert_der: &[u8]) -> Result<String, McpixError> {
    use x509_parser::prelude::*;

    let (_, cert) = X509Certificate::from_der(cert_der)
        .map_err(|e| McpixError::Transport(format!("parse cert: {e}")))?;

    // 1) Tenta extensão SAN.
    if let Ok(Some(san_ext)) = cert.subject_alternative_name() {
        for name in &san_ext.value.general_names {
            if let GeneralName::URI(uri) = name {
                if let Some(rest) = uri.strip_prefix(INSTITUTION_URI_PREFIX) {
                    return Ok(rest.to_string());
                }
            }
        }
    }

    // 2) Fallback: CN do subject.
    for rdn in cert.subject().iter_common_name() {
        if let Ok(cn) = rdn.as_str() {
            return Ok(cn.to_string());
        }
    }

    Err(McpixError::Transport(
        "client cert lacks SAN URI and CN — cannot determine institution".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cert_with_uri_san(uri: &str) -> Vec<u8> {
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        params.subject_alt_names = vec![rcgen::SanType::URI(uri.try_into().unwrap())];
        let kp = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&kp).unwrap();
        cert.der().to_vec()
    }

    fn make_cert_with_cn_only(cn: &str) -> Vec<u8> {
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, cn);
        params.distinguished_name = dn;
        let kp = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&kp).unwrap();
        cert.der().to_vec()
    }

    #[test]
    fn extracts_institution_from_san_uri() {
        let der = make_cert_with_uri_san("urn:mcpix:institution:BANK_PAYER_42");
        assert_eq!(
            extract_institution_id(&der).unwrap(),
            "BANK_PAYER_42"
        );
    }

    #[test]
    fn falls_back_to_cn_when_no_san_uri() {
        let der = make_cert_with_cn_only("legacy-bank");
        assert_eq!(extract_institution_id(&der).unwrap(), "legacy-bank");
    }

    fn make_cert_with_uri_san_and_cn(uri: &str, cn: &str) -> Vec<u8> {
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        params.subject_alt_names = vec![rcgen::SanType::URI(uri.try_into().unwrap())];
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, cn);
        params.distinguished_name = dn;
        let kp = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&kp).unwrap();
        cert.der().to_vec()
    }

    #[test]
    fn ignores_san_uri_without_correct_prefix_and_falls_back_to_cn() {
        // SAN URI presente mas com prefixo errado → deve cair no CN.
        let der = make_cert_with_uri_san_and_cn("urn:other:thing:foo", "fallback-cn");
        assert_eq!(extract_institution_id(&der).unwrap(), "fallback-cn");
    }
}
