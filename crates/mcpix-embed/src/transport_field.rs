//! Encode/parse do campo de transporte (35 chars `[a-zA-Z0-9]`) sem alloc.
//!
//! API receiver-only: o dispositivo embarcado só **emite** o campo (para
//! mostrar como QR / texto no display). O parse fica disponível para casos
//! onde o microcontrolador também lê o próprio campo de volta (loopback,
//! diagnóstico).

use crate::types::{EmbedError, SeedId, C1, C1_LEN, SEED_ID_MAX_LEN};

pub const PROTOCOL_PREFIX: &str = "PIXOFFv1";
pub const PROTOCOL_PREFIX_LEN: usize = 8;
pub const TRANSPORT_FIELD_LEN: usize = PROTOCOL_PREFIX_LEN + SEED_ID_MAX_LEN + C1_LEN;

const SEED_ID_PAD: u8 = b'0';

/// Serializa `(seed_id, c1)` em `out` (35 bytes fixos). Devolve `&str` para
/// o conteúdo gravado.
pub fn encode_into<'a>(
    seed_id: &SeedId,
    c1: &C1,
    out: &'a mut [u8; TRANSPORT_FIELD_LEN],
) -> &'a str {
    out[..PROTOCOL_PREFIX_LEN].copy_from_slice(PROTOCOL_PREFIX.as_bytes());

    let sid_bytes = seed_id.as_bytes();
    let sid_start = PROTOCOL_PREFIX_LEN;
    let sid_end = sid_start + sid_bytes.len();
    out[sid_start..sid_end].copy_from_slice(sid_bytes);

    // Padding com '0' até completar o slot.
    for slot in &mut out[sid_end..(PROTOCOL_PREFIX_LEN + SEED_ID_MAX_LEN)] {
        *slot = SEED_ID_PAD;
    }

    let c1_start = PROTOCOL_PREFIX_LEN + SEED_ID_MAX_LEN;
    out[c1_start..].copy_from_slice(c1.as_bytes());

    core::str::from_utf8(out).expect("encoded field is ASCII by construction")
}

/// Saída de `parse_into` — referências aos slots dentro do buffer recebido.
#[derive(Debug, Clone)]
pub struct ParsedField {
    pub seed_id: SeedId,
    pub c1: C1,
}

pub fn parse_into(field: &str) -> Result<ParsedField, EmbedError> {
    let bytes = field.as_bytes();
    if bytes.len() != TRANSPORT_FIELD_LEN {
        return Err(EmbedError::BufferLen {
            expected: TRANSPORT_FIELD_LEN,
            got: bytes.len(),
        });
    }
    if !bytes.iter().all(|b| b.is_ascii_alphanumeric()) {
        return Err(EmbedError::TransportFieldLayout);
    }
    if &bytes[..PROTOCOL_PREFIX_LEN] != PROTOCOL_PREFIX.as_bytes() {
        return Err(EmbedError::TransportFieldLayout);
    }

    let sid_slot = &bytes[PROTOCOL_PREFIX_LEN..PROTOCOL_PREFIX_LEN + SEED_ID_MAX_LEN];
    // Remove o padding '0' da direita.
    let mut sid_end = sid_slot.len();
    while sid_end > 0 && sid_slot[sid_end - 1] == SEED_ID_PAD {
        sid_end -= 1;
    }
    let sid_str =
        core::str::from_utf8(&sid_slot[..sid_end]).map_err(|_| EmbedError::TransportFieldLayout)?;
    let seed_id = SeedId::new(sid_str)?;

    let mut c1_buf = [0u8; C1_LEN];
    c1_buf.copy_from_slice(&bytes[PROTOCOL_PREFIX_LEN + SEED_ID_MAX_LEN..]);
    let c1 = C1(c1_buf);

    Ok(ParsedField { seed_id, c1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::derive_pair;
    use crate::types::Seed;

    #[test]
    fn encode_then_parse_roundtrip() {
        let sid = SeedId::new("R1").unwrap();
        let seed = Seed::from_bytes([0x42; 32]);
        let (c1, _) = derive_pair(&seed, 1);
        let mut buf = [0u8; TRANSPORT_FIELD_LEN];
        let field = encode_into(&sid, &c1, &mut buf);
        assert_eq!(field.len(), 35);
        assert!(field.starts_with("PIXOFFv1"));
        let parsed = parse_into(field).unwrap();
        assert_eq!(parsed.seed_id.as_str(), "R1");
        assert_eq!(parsed.c1.as_str(), c1.as_str());
    }
}
