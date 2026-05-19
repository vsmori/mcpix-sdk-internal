//! Primitivas criptográficas — réplica algorítmica de `mcpix-core::crypto`
//! sem `String`/`Vec` e sem allocator.
//!
//! O **algoritmo é idêntico** ao do host. `tests/cross_validate.rs` confirma
//! que para todo `(seed, counter)` o output destes funções coincide byte-a-byte
//! com `mcpix-core::crypto::derive_pair`. Qualquer drift quebra a validação.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::types::{Seed, C1, C1_LEN, C2, C2_LEN};

type HmacSha256 = Hmac<Sha256>;

/// Alfabeto base32-Crockford-like alfanumérico ASCII. Mesma constante do host.
const ALPHANUMERIC: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

const DOMAIN_C1: &[u8] = b"mcpix/v1/c1";
const DOMAIN_C2: &[u8] = b"mcpix/v1/c2";

/// Encode 5-bit-per-char no alfabeto custom para `out_len` chars.
fn encode_alphanumeric<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut out = [b'A'; N];
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut byte_idx = 0;
    for slot in out.iter_mut() {
        while bits < 5 {
            acc = (acc << 8) | bytes[byte_idx] as u32;
            byte_idx += 1;
            bits += 8;
        }
        let idx = ((acc >> (bits - 5)) & 0b11111) as usize;
        bits -= 5;
        acc &= (1 << bits) - 1;
        *slot = ALPHANUMERIC[idx];
    }
    out
}

fn hmac_chunks(seed: &Seed, chunks: &[&[u8]]) -> [u8; 32] {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(seed.as_bytes()).expect("HMAC accepts any key length");
    for c in chunks {
        mac.update(c);
    }
    let out = mac.finalize().into_bytes();
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&out);
    buf
}

/// `C₁ = trunc(HMAC(S, "mcpix/v1/c1" || T_be))`, `C₂ = trunc(HMAC(S, "mcpix/v1/c2" || T_be || C₁))`.
pub fn derive_pair(seed: &Seed, counter: u64) -> (C1, C2) {
    let t = counter.to_be_bytes();

    let c1_mac = hmac_chunks(seed, &[DOMAIN_C1, &t]);
    let c1_chars: [u8; C1_LEN] = encode_alphanumeric(&c1_mac);
    let c1 = C1(c1_chars);

    let c2 = derive_c2_from_c1(seed, counter, &c1);
    (c1, c2)
}

/// Recompõe `C₂` dado `C₁` — útil quando o dispositivo precisa validar
/// localmente um C₂ apresentado por outro caminho.
pub fn derive_c2_from_c1(seed: &Seed, counter: u64, c1: &C1) -> C2 {
    let t = counter.to_be_bytes();
    let c2_mac = hmac_chunks(seed, &[DOMAIN_C2, &t, c1.as_bytes()]);
    let c2_chars: [u8; C2_LEN] = encode_alphanumeric(&c2_mac);
    C2(c2_chars)
}

/// Comparação em tempo constante. Mesma garantia do host.
pub fn verify_c2(expected: &C2, presented: &C2) -> bool {
    expected.0.ct_eq(&presented.0).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_on_target() {
        let s = Seed::from_bytes([0xAB; 32]);
        let (a1, a2) = derive_pair(&s, 42);
        let (b1, b2) = derive_pair(&s, 42);
        assert_eq!(a1.as_str(), b1.as_str());
        assert_eq!(a2.as_str(), b2.as_str());
    }

    #[test]
    fn output_is_alphanumeric_and_correct_length() {
        let s = Seed::from_bytes([0x55; 32]);
        let (c1, c2) = derive_pair(&s, 1234567);
        assert_eq!(c1.as_str().len(), C1_LEN);
        assert_eq!(c2.as_str().len(), C2_LEN);
        assert!(c1.as_str().bytes().all(|b| b.is_ascii_alphanumeric()));
        assert!(c2.as_str().bytes().all(|b| b.is_ascii_alphanumeric()));
    }
}
