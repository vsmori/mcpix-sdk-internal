//! Campo de transporte público (`[a-zA-Z0-9]{26,35}`).
//!
//! Layout fixo escolhido para a demo de referência:
//!
//! ```text
//!   ┌──────────┬──────────────────┬─────────────┐
//!   │ PREFIX 8 │ SeedId 16 (pad)  │ C1 11 chars │
//!   └──────────┴──────────────────┴─────────────┘
//!         8     +        16         +     11    = 35 chars
//! ```
//!
//! O SeedId é pad-right com `'0'` até completar 16 chars (zero ASCII é alfanumérico
//! e fora do alfabeto C₁/C₂ em maiúsculas — facilita inspeção visual). O prefixo
//! `PIXOFFv1` é constante de versão de protocolo e o único ponto que carrega
//! identidade do esquema; deriva-se dele a sinalização inter-institucional.

use crate::error::McpixError;
use crate::types::{C1, C1_TRANSPORT_LEN, SEED_ID_MAX_LEN, SeedId};

pub const PROTOCOL_PREFIX: &str = "PIXOFFv1";
pub const PROTOCOL_PREFIX_LEN: usize = 8;
pub const TRANSPORT_FIELD_LEN: usize = PROTOCOL_PREFIX_LEN + SEED_ID_MAX_LEN + C1_TRANSPORT_LEN;

const SEED_ID_PAD: u8 = b'0';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedField {
    pub seed_id: SeedId,
    pub c1: C1,
}

/// Serializa `(seed_id, c1)` no campo de transporte público.
pub fn encode(seed_id: &SeedId, c1: &C1) -> String {
    let mut out = String::with_capacity(TRANSPORT_FIELD_LEN);
    out.push_str(PROTOCOL_PREFIX);
    out.push_str(seed_id.as_str());
    // Padding até completar SEED_ID_MAX_LEN. Sem isso a parsing por posição falha
    // quando o SeedId for mais curto que o slot reservado.
    for _ in seed_id.as_str().len()..SEED_ID_MAX_LEN {
        out.push(SEED_ID_PAD as char);
    }
    out.push_str(c1.as_str());
    debug_assert_eq!(out.len(), TRANSPORT_FIELD_LEN);
    out
}

/// Faz parse e valida o campo de transporte. Retorna `(SeedId, C₁)` extraídos.
pub fn parse(field: &str) -> Result<ParsedField, McpixError> {
    let len = field.len();
    if !(26..=35).contains(&len) {
        return Err(McpixError::TransportFieldLength(len));
    }
    // Garantia universal antes do parsing por posição: o campo inteiro só pode
    // conter [a-zA-Z0-9]. Falhar cedo evita interpretar lixo binário como SeedId.
    for (i, b) in field.bytes().enumerate() {
        if !b.is_ascii_alphanumeric() {
            return Err(McpixError::TransportFieldCharset(i));
        }
    }
    if len != TRANSPORT_FIELD_LEN {
        // Layout v1 é fixo em 35; faixa 26-35 é a janela do protocolo público,
        // não da nossa versão. Outras versões usariam outro PROTOCOL_PREFIX.
        return Err(McpixError::TransportFieldLength(len));
    }
    if !field.starts_with(PROTOCOL_PREFIX) {
        return Err(McpixError::TransportFieldPrefix);
    }

    let seed_id_slot = &field[PROTOCOL_PREFIX_LEN..PROTOCOL_PREFIX_LEN + SEED_ID_MAX_LEN];
    let seed_id_str = seed_id_slot.trim_end_matches(SEED_ID_PAD as char);
    let seed_id = SeedId::new(seed_id_str.to_string())?;

    let c1_slot = &field[PROTOCOL_PREFIX_LEN + SEED_ID_MAX_LEN..];
    let mut c1_bytes = [0u8; C1_TRANSPORT_LEN];
    c1_bytes.copy_from_slice(c1_slot.as_bytes());
    let c1 = C1(c1_bytes);

    Ok(ParsedField { seed_id, c1 })
}

/// Detecta se uma string carrega o nosso esquema. Usado pelo banco do pagador
/// como triagem rápida antes do parse completo.
pub fn is_protocol_field(field: &str) -> bool {
    field.len() == TRANSPORT_FIELD_LEN && field.starts_with(PROTOCOL_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::derive_pair;
    use crate::types::Seed;

    fn sample_seed_id() -> SeedId {
        SeedId::new("RECVR1").unwrap()
    }

    #[test]
    fn round_trip_preserves_fields() {
        let seed = Seed::from_bytes([0x42; 32]);
        let (c1, _) = derive_pair(&seed, 1);
        let field = encode(&sample_seed_id(), &c1);
        assert_eq!(field.len(), TRANSPORT_FIELD_LEN);
        let parsed = parse(&field).unwrap();
        assert_eq!(parsed.seed_id, sample_seed_id());
        assert_eq!(parsed.c1, c1);
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(matches!(
            parse("short"),
            Err(McpixError::TransportFieldLength(_))
        ));
    }

    #[test]
    fn rejects_non_alphanumeric() {
        let mut s = String::from("PIXOFFv1RECVR01-----0AAAAAAAAAAAAA");
        // Garante o tamanho 35 com um '-' inserido.
        while s.len() < TRANSPORT_FIELD_LEN {
            s.push('A');
        }
        s.truncate(TRANSPORT_FIELD_LEN);
        // Força um caractere inválido na posição 14.
        let mut bytes = s.into_bytes();
        bytes[14] = b'-';
        let s = String::from_utf8(bytes).unwrap();
        assert!(matches!(
            parse(&s),
            Err(McpixError::TransportFieldCharset(14))
        ));
    }

    #[test]
    fn rejects_wrong_prefix() {
        let mut s = String::from("XXXXXXXXRECVR01000000000AAAAAAAAAAAA");
        s.truncate(TRANSPORT_FIELD_LEN);
        assert!(matches!(parse(&s), Err(McpixError::TransportFieldPrefix)));
    }

    #[test]
    fn seed_id_with_internal_digits_roundtrips() {
        // Caso que quebraria se permitíssemos '0' no alfabeto do SeedId:
        // "R12" não pode ser confundido com "R12" + pad zero. Como excluímos
        // '0' do alfabeto via SeedId::new, qualquer dígito interno é seguro.
        let sid = SeedId::new("R12").unwrap();
        let seed = Seed::from_bytes([0x42; 32]);
        let (c1, _) = derive_pair(&seed, 1);
        let field = encode(&sid, &c1);
        let parsed = parse(&field).unwrap();
        assert_eq!(parsed.seed_id.as_str(), "R12");
    }

    #[test]
    fn seed_id_with_zero_is_rejected() {
        assert!(SeedId::new("R10").is_err());
        assert!(SeedId::new("R01").is_err());
    }

    #[test]
    fn is_protocol_field_detects_prefix() {
        let seed = Seed::from_bytes([1; 32]);
        let (c1, _) = derive_pair(&seed, 1);
        let field = encode(&sample_seed_id(), &c1);
        assert!(is_protocol_field(&field));
        assert!(!is_protocol_field("RANDOM_TXID_NOT_OURS_00000000000000"));
    }
}
