//! Restore embarcado de backup criptografado.
//!
//! **Wire format idêntico** ao da crate `mcpix-backup` (host). Caminho
//! de leitura paralelo, com tipos `no_std` (`heapless::Vec`,
//! `mcpix-embed::types`). Cross-validado por teste host-side em
//! `tests/backup_cross_validate.rs`.
//!
//! ## Pré-requisitos
//!
//! Esta feature usa `argon2`, `chacha20poly1305` e `bs58`, todos os
//! quais consomem `alloc::Vec` internamente. **Caller precisa fornecer
//! um global allocator**:
//!
//! - ESP32 family: `esp-alloc`
//! - Cortex-M: `linked_list_allocator` ou `embedded-alloc`
//! - ESP8266: **não recomendado** — Argon2id m=64 KiB consome 1/3 da
//!   RAM disponível; fluxo de restore pode estourar.
//!
//! ## API mínima
//!
//! ```ignore
//! let restored = mcpix_embed::restore::import(backup_text, b"passphrase")?;
//! // restored.seed, restored.seed_id, restored.counter_t prontos para
//! // serem persistidos via mcpix_embed::storage.
//! ```

extern crate alloc;

use alloc::vec::Vec;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use zeroize::Zeroize;

use crate::types::{Seed, SeedId, SEED_ID_MAX_LEN, SEED_LEN};

// ─────────────────────────────────────────────────────────────────────────────
// Constantes do wire format — DEVEM bater com mcpix-backup::format
// ─────────────────────────────────────────────────────────────────────────────

const MAGIC: [u8; 4] = *b"MBKP";
const VERSION: u16 = 0x0001;
const KDF_ARGON2ID: u8 = 1;
const PAYLOAD_VERSION: u8 = 0x01;

const HEADER_LEN: usize = 47;
const PAYLOAD_LEN: usize = 59;
const ENCRYPTED_PAYLOAD_LEN: usize = PAYLOAD_LEN + 16; // +tag Poly1305
const TOTAL_LEN: usize = HEADER_LEN + ENCRYPTED_PAYLOAD_LEN;

// ─────────────────────────────────────────────────────────────────────────────
// Tipos públicos
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterMode {
    Sequential = 0,
    Quantized = 1,
}

impl CounterMode {
    fn from_u8(b: u8) -> Result<Self, RestoreError> {
        match b {
            0 => Ok(Self::Sequential),
            1 => Ok(Self::Quantized),
            _ => Err(RestoreError::Malformed),
        }
    }
}

#[derive(Debug)]
pub struct RestoredState {
    pub seed: Seed,
    pub seed_id: SeedId,
    pub counter_mode: CounterMode,
    pub counter_t: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreError {
    Malformed,
    UnsupportedVersion,
    DecryptFailed,
    KdfFailed,
    InvalidSeedId,
}

// ─────────────────────────────────────────────────────────────────────────────
// API
// ─────────────────────────────────────────────────────────────────────────────

pub fn import(text: &str, passphrase: &[u8]) -> Result<RestoredState, RestoreError> {
    // 1. Base58Check decode (verifica integridade básica via checksum SHA-256x2).
    let blob: Vec<u8> = bs58::decode(text.as_bytes())
        .with_check(None)
        .into_vec()
        .map_err(|_| RestoreError::Malformed)?;
    if blob.len() != TOTAL_LEN {
        return Err(RestoreError::Malformed);
    }

    // 2. Parse header (AAD para AEAD).
    let header: &[u8; HEADER_LEN] = blob[..HEADER_LEN]
        .try_into()
        .map_err(|_| RestoreError::Malformed)?;
    let HeaderFields {
        m_cost_kib,
        t_cost,
        p_cost,
        salt,
        nonce_bytes,
    } = parse_header(header)?;

    // 3. KDF.
    let mut key_bytes = [0u8; 32];
    derive_argon2id(
        passphrase,
        &salt,
        m_cost_kib,
        t_cost,
        p_cost,
        &mut key_bytes,
    )?;

    // 4. Decrypt com AAD = header.
    let key = Key::from_slice(&key_bytes);
    let cipher = ChaCha20Poly1305::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(
            nonce,
            Payload {
                msg: &blob[HEADER_LEN..],
                aad: header.as_slice(),
            },
        )
        .map_err(|_| RestoreError::DecryptFailed)?;
    key_bytes.zeroize();

    // 5. Parse payload.
    if plaintext.len() != PAYLOAD_LEN {
        return Err(RestoreError::Malformed);
    }
    let payload: &[u8; PAYLOAD_LEN] = plaintext
        .as_slice()
        .try_into()
        .map_err(|_| RestoreError::Malformed)?;
    parse_payload(payload)
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

struct HeaderFields {
    m_cost_kib: u32,
    t_cost: u32,
    p_cost: u8,
    salt: [u8; 16],
    nonce_bytes: [u8; 12],
}

fn parse_header(buf: &[u8; HEADER_LEN]) -> Result<HeaderFields, RestoreError> {
    if buf[0..4] != MAGIC {
        return Err(RestoreError::Malformed);
    }
    let version = u16::from_be_bytes([buf[4], buf[5]]);
    if version != VERSION {
        return Err(RestoreError::UnsupportedVersion);
    }
    if buf[6] != KDF_ARGON2ID {
        return Err(RestoreError::UnsupportedVersion);
    }
    let m_cost_kib = u32::from_be_bytes([buf[7], buf[8], buf[9], buf[10]]);
    let t_cost = u32::from_be_bytes([buf[11], buf[12], buf[13], buf[14]]);
    let p_cost = buf[15];
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&buf[19..35]);
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes.copy_from_slice(&buf[35..47]);
    Ok(HeaderFields {
        m_cost_kib,
        t_cost,
        p_cost,
        salt,
        nonce_bytes,
    })
}

fn derive_argon2id(
    passphrase: &[u8],
    salt: &[u8; 16],
    m_cost_kib: u32,
    t_cost: u32,
    p_cost: u8,
    out: &mut [u8; 32],
) -> Result<(), RestoreError> {
    let params = Params::new(m_cost_kib, t_cost, p_cost as u32, Some(32))
        .map_err(|_| RestoreError::KdfFailed)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon
        .hash_password_into(passphrase, salt, out)
        .map_err(|_| RestoreError::KdfFailed)
}

fn parse_payload(buf: &[u8; PAYLOAD_LEN]) -> Result<RestoredState, RestoreError> {
    if buf[0] != PAYLOAD_VERSION {
        return Err(RestoreError::UnsupportedVersion);
    }
    let mut seed_bytes = [0u8; SEED_LEN];
    seed_bytes.copy_from_slice(&buf[1..1 + SEED_LEN]);
    let seed = Seed::from_bytes(seed_bytes);

    let sid_len = buf[1 + SEED_LEN] as usize;
    if sid_len == 0 || sid_len > SEED_ID_MAX_LEN {
        return Err(RestoreError::InvalidSeedId);
    }
    let sid_off = 1 + SEED_LEN + 1;
    let sid_str = core::str::from_utf8(&buf[sid_off..sid_off + sid_len])
        .map_err(|_| RestoreError::InvalidSeedId)?;
    let seed_id = SeedId::new(sid_str).map_err(|_| RestoreError::InvalidSeedId)?;

    let mode_off = sid_off + SEED_ID_MAX_LEN;
    let counter_mode = CounterMode::from_u8(buf[mode_off])?;
    let mut t_bytes = [0u8; 8];
    t_bytes.copy_from_slice(&buf[mode_off + 1..mode_off + 9]);
    let counter_t = u64::from_be_bytes(t_bytes);

    Ok(RestoredState {
        seed,
        seed_id,
        counter_mode,
        counter_t,
    })
}
