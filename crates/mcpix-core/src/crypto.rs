//! Primitivas criptográficas determinísticas e de tempo constante.
//!
//! Toda derivação parte de `HMAC-SHA-256(S, ·)` — uma função unidirecional cuja
//! propriedade essencial aqui é o **determinismo**: o banco do pagador, de posse
//! da mesma semente `S` e do mesmo contador `T`, recompõe `C₂` exatamente.
//! É essa substituição institucional que o protocolo se apoia.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::types::{C1, C1_TRANSPORT_LEN, C2, C2_TRANSPORT_LEN, Seed};

type HmacSha256 = Hmac<Sha256>;

/// Alfabeto base32 customizado, alfanumérico ASCII, sem caracteres ambíguos do RFC 4648
/// que escapariam da faixa `[A-Z0-9]`. Combinado com a faixa `[a-zA-Z0-9]` do campo de
/// transporte, mantemos o subconjunto em maiúsculas para previsibilidade visual.
const ALPHANUMERIC: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Codifica `bytes` em string alfanumérica de tamanho exato `out_len` aplicando
/// o alfabeto custom. Cada char carrega 5 bits, então o consumo é `ceil(out_len*5/8)`
/// bytes do MAC; o resto do MAC é descartado.
fn encode_alphanumeric(bytes: &[u8], out_len: usize) -> [u8; 16] {
    debug_assert!(out_len <= 16);
    let mut out = [b'A'; 16];
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut byte_idx = 0;
    for slot in out.iter_mut().take(out_len) {
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

/// HMAC-SHA-256 sobre uma sequência de chunks. Aceitar chunks permite domain
/// separation sem alocar buffers intermediários.
fn hmac_chunks(seed: &Seed, chunks: &[&[u8]]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(seed.as_bytes())
        .expect("HMAC-SHA-256 accepts any key length");
    for c in chunks {
        mac.update(c);
    }
    let out = mac.finalize().into_bytes();
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&out);
    buf
}

/// Tag de domain separation: distingue derivações C₁ vs C₂ a partir do mesmo `S`.
/// Sem isso, um atacante poderia confundir os papéis ao reutilizar a função.
const DOMAIN_C1: &[u8] = b"mcpix/v1/c1";
const DOMAIN_C2: &[u8] = b"mcpix/v1/c2";

/// Deriva o par atômico `(C₁, C₂)` a partir de `(S, T)`.
///
/// - `C₁ = trunc( HMAC(S, "mcpix/v1/c1" || T_be) )`
/// - `C₂ = trunc( HMAC(S, "mcpix/v1/c2" || T_be || C₁) )` — encadeamento explícito
///
/// O encadeamento `C₂ = f(S, T, C₁)` garante que C₂ não pode ser derivado sem
/// conhecer C₁ — propriedade exigida pelo protocolo (par atômico encadeado).
pub fn derive_pair(seed: &Seed, counter: u64) -> (C1, C2) {
    let t = counter.to_be_bytes();

    let c1_mac = hmac_chunks(seed, &[DOMAIN_C1, &t]);
    let c1_chars = encode_alphanumeric(&c1_mac, C1_TRANSPORT_LEN);
    let mut c1 = [0u8; C1_TRANSPORT_LEN];
    c1.copy_from_slice(&c1_chars[..C1_TRANSPORT_LEN]);
    let c1 = C1(c1);

    let c2 = derive_c2_from_c1(seed, counter, &c1);
    (c1, c2)
}

/// Deriva apenas `C₂` dado um `C₁` recebido (caminho do banco do pagador, que
/// extrai `C₁` do campo de transporte e reconstrói `C₂`).
pub fn derive_c2_from_c1(seed: &Seed, counter: u64, c1: &C1) -> C2 {
    let t = counter.to_be_bytes();
    let c2_mac = hmac_chunks(seed, &[DOMAIN_C2, &t, c1.as_str().as_bytes()]);
    let c2_chars = encode_alphanumeric(&c2_mac, C2_TRANSPORT_LEN);
    let mut c2 = [0u8; C2_TRANSPORT_LEN];
    c2.copy_from_slice(&c2_chars[..C2_TRANSPORT_LEN]);
    C2(c2)
}

/// Comparação em **tempo constante** entre dois `C₂`.
///
/// Por que importa: `==` em string termina no primeiro byte divergente. Um
/// atacante que controla o `C₂` apresentado e mede latência consegue inferir
/// prefixos do `C₂` esperado, byte a byte (ataque de canal lateral por timing).
/// `subtle::ConstantTimeEq` mantém o tempo de execução independente do conteúdo.
pub fn verify_c2(expected: &C2, presented: &C2) -> bool {
    expected.0.ct_eq(&presented.0).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_of(byte: u8) -> Seed {
        Seed::from_bytes([byte; 32])
    }

    #[test]
    fn derive_pair_is_deterministic() {
        let s = seed_of(0xA5);
        let (c1a, c2a) = derive_pair(&s, 42);
        let (c1b, c2b) = derive_pair(&s, 42);
        assert_eq!(c1a.as_str(), c1b.as_str());
        assert_eq!(c2a.as_str(), c2b.as_str());
    }

    #[test]
    fn different_counter_yields_different_pair() {
        let s = seed_of(0xA5);
        let (c1a, _) = derive_pair(&s, 1);
        let (c1b, _) = derive_pair(&s, 2);
        assert_ne!(c1a.as_str(), c1b.as_str());
    }

    #[test]
    fn different_seed_yields_different_pair() {
        let (c1a, _) = derive_pair(&seed_of(0x01), 1);
        let (c1b, _) = derive_pair(&seed_of(0x02), 1);
        assert_ne!(c1a.as_str(), c1b.as_str());
    }

    #[test]
    fn c2_derives_from_c1_consistently() {
        let s = seed_of(0x77);
        let (c1, c2_full) = derive_pair(&s, 9001);
        let c2_recon = derive_c2_from_c1(&s, 9001, &c1);
        assert_eq!(c2_full.as_str(), c2_recon.as_str());
    }

    #[test]
    fn verify_accepts_match() {
        let s = seed_of(0x10);
        let (_, c2) = derive_pair(&s, 7);
        assert!(verify_c2(&c2, &c2.clone()));
    }

    #[test]
    fn verify_rejects_mismatch() {
        let s = seed_of(0x10);
        let (_, c2a) = derive_pair(&s, 7);
        let (_, c2b) = derive_pair(&s, 8);
        assert!(!verify_c2(&c2a, &c2b));
    }

    #[test]
    fn encoded_chars_are_alphanumeric() {
        let s = seed_of(0xFF);
        let (c1, c2) = derive_pair(&s, 1234567);
        assert!(c1.as_str().bytes().all(|b| b.is_ascii_alphanumeric()));
        assert!(c2.as_str().bytes().all(|b| b.is_ascii_alphanumeric()));
        assert_eq!(c1.as_str().len(), C1_TRANSPORT_LEN);
        assert_eq!(c2.as_str().len(), C2_TRANSPORT_LEN);
    }
}
