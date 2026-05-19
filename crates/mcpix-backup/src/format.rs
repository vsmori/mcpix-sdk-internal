//! Serialização do header (cleartext + AAD) e do payload (cleartext
//! pré-AEAD). Tudo big-endian, sem alignment requirements.

use mcpix_core::types::{Seed, SeedId, SEED_ID_MAX_LEN, SEED_LEN};

use crate::{BackupError, ExportInput, RestoredState};

pub const MAGIC: [u8; 4] = *b"MBKP";
pub const VERSION: u16 = 0x0001;
const KDF_ARGON2ID: u8 = 1;

pub const HEADER_LEN: usize = 47;
pub const PAYLOAD_LEN: usize = 59;
pub const ENCRYPTED_PAYLOAD_LEN: usize = PAYLOAD_LEN + 16; // +tag Poly1305
pub const TOTAL_LEN: usize = HEADER_LEN + ENCRYPTED_PAYLOAD_LEN;

const _: () = assert!(HEADER_LEN == 4 + 2 + 1 + 4 + 4 + 1 + 3 + 16 + 12);
const _: () = assert!(PAYLOAD_LEN == 1 + SEED_LEN + 1 + SEED_ID_MAX_LEN + 1 + 8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterMode {
    Sequential = 0,
    Quantized = 1,
}

impl CounterMode {
    fn from_u8(b: u8) -> Result<Self, BackupError> {
        match b {
            0 => Ok(Self::Sequential),
            1 => Ok(Self::Quantized),
            _ => Err(BackupError::Malformed),
        }
    }
}

/// Parâmetros KDF embarcados no header. Detalhe importante: como o
/// header é **AAD da AEAD**, qualquer atacante que tente reduzir
/// `m_cost` para acelerar brute-force quebra a tag → restore falha.
#[derive(Debug, Clone, Copy)]
pub struct KdfParams {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u8,
}

impl KdfParams {
    /// Default OWASP 2024 "Argon2id sensitive": 64 MiB, 3 iter, p=1.
    /// Verificação consome ~300ms em laptop moderno.
    pub const fn default_strong() -> Self {
        Self {
            m_cost_kib: 64 * 1024,
            t_cost: 3,
            p_cost: 1,
        }
    }

    /// Default reduzido para MCU restaurando o backup: 64 KiB, 3 iter,
    /// p=1. Verificação consome ~1.5s em Cortex-M4F @72MHz — aceitável
    /// para fluxo de restore one-time.
    pub const fn default_embed() -> Self {
        Self {
            m_cost_kib: 64,
            t_cost: 3,
            p_cost: 1,
        }
    }
}

pub fn write_header(
    buf: &mut [u8; HEADER_LEN],
    params: &KdfParams,
    salt: &[u8; 16],
    nonce: &[u8; 12],
) {
    buf[0..4].copy_from_slice(&MAGIC);
    buf[4..6].copy_from_slice(&VERSION.to_be_bytes());
    buf[6] = KDF_ARGON2ID;
    buf[7..11].copy_from_slice(&params.m_cost_kib.to_be_bytes());
    buf[11..15].copy_from_slice(&params.t_cost.to_be_bytes());
    buf[15] = params.p_cost;
    buf[16..19].fill(0); // reserved
    buf[19..35].copy_from_slice(salt);
    buf[35..47].copy_from_slice(nonce);
}

pub fn read_header(buf: &[u8; HEADER_LEN]) -> Result<(KdfParams, [u8; 16], [u8; 12]), BackupError> {
    if buf[0..4] != MAGIC {
        return Err(BackupError::Malformed);
    }
    let version = u16::from_be_bytes([buf[4], buf[5]]);
    if version != VERSION {
        return Err(BackupError::UnsupportedVersion);
    }
    if buf[6] != KDF_ARGON2ID {
        return Err(BackupError::UnsupportedVersion);
    }
    let m_cost_kib = u32::from_be_bytes([buf[7], buf[8], buf[9], buf[10]]);
    let t_cost = u32::from_be_bytes([buf[11], buf[12], buf[13], buf[14]]);
    let p_cost = buf[15];
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&buf[19..35]);
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&buf[35..47]);
    Ok((
        KdfParams {
            m_cost_kib,
            t_cost,
            p_cost,
        },
        salt,
        nonce,
    ))
}

pub fn write_payload(buf: &mut [u8; PAYLOAD_LEN], input: &ExportInput<'_>) {
    buf[0] = 0x01; // payload_version
    buf[1..1 + SEED_LEN].copy_from_slice(input.seed.as_bytes());

    let sid = input.seed_id.as_str().as_bytes();
    buf[1 + SEED_LEN] = sid.len() as u8;
    let sid_off = 1 + SEED_LEN + 1;
    buf[sid_off..sid_off + sid.len()].copy_from_slice(sid);
    // Zero-pad o resto do slot do SeedId (16 bytes).
    for byte in &mut buf[sid_off + sid.len()..sid_off + SEED_ID_MAX_LEN] {
        *byte = 0;
    }

    let mode_off = sid_off + SEED_ID_MAX_LEN;
    buf[mode_off] = input.counter_mode as u8;
    buf[mode_off + 1..mode_off + 9].copy_from_slice(&input.counter_t.to_be_bytes());
}

/// View sobre o payload em claro recém-decifrado. Mantém referência
/// para que o caller possa zeroizar o buffer original quando terminar.
pub struct PayloadView<'a> {
    bytes: &'a [u8; PAYLOAD_LEN],
}

impl<'a> PayloadView<'a> {
    pub fn parse(bytes: &'a [u8; PAYLOAD_LEN]) -> Result<Self, BackupError> {
        if bytes[0] != 0x01 {
            return Err(BackupError::UnsupportedVersion);
        }
        Ok(Self { bytes })
    }

    pub fn into_state(self) -> Result<RestoredState, BackupError> {
        let mut seed_bytes = [0u8; SEED_LEN];
        seed_bytes.copy_from_slice(&self.bytes[1..1 + SEED_LEN]);
        let seed = Seed::from_bytes(seed_bytes);

        let sid_len = self.bytes[1 + SEED_LEN] as usize;
        if sid_len == 0 || sid_len > SEED_ID_MAX_LEN {
            return Err(BackupError::InvalidSeedId);
        }
        let sid_off = 1 + SEED_LEN + 1;
        let sid_str = core::str::from_utf8(&self.bytes[sid_off..sid_off + sid_len])
            .map_err(|_| BackupError::InvalidSeedId)?;
        let seed_id = SeedId::new(sid_str).map_err(|_| BackupError::InvalidSeedId)?;

        let mode_off = sid_off + SEED_ID_MAX_LEN;
        let counter_mode = CounterMode::from_u8(self.bytes[mode_off])?;
        let mut t_bytes = [0u8; 8];
        t_bytes.copy_from_slice(&self.bytes[mode_off + 1..mode_off + 9]);
        let counter_t = u64::from_be_bytes(t_bytes);

        Ok(RestoredState {
            seed,
            seed_id,
            counter_mode,
            counter_t,
        })
    }
}

/// Container só serve como type alias documental — o blob real é
/// `[u8; TOTAL_LEN]`. Mantido para clareza em assinaturas de funções
/// internas que circulam o array por ref.
pub type Container = [u8; TOTAL_LEN];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let params = KdfParams {
            m_cost_kib: 65536,
            t_cost: 3,
            p_cost: 1,
        };
        let salt = [0x55u8; 16];
        let nonce = [0xAAu8; 12];
        let mut buf = [0u8; HEADER_LEN];
        write_header(&mut buf, &params, &salt, &nonce);
        let (p2, s2, n2) = read_header(&buf).unwrap();
        assert_eq!(p2.m_cost_kib, params.m_cost_kib);
        assert_eq!(p2.t_cost, params.t_cost);
        assert_eq!(p2.p_cost, params.p_cost);
        assert_eq!(s2, salt);
        assert_eq!(n2, nonce);
    }

    #[test]
    fn header_rejects_bad_magic() {
        let mut buf = [0u8; HEADER_LEN];
        write_header(&mut buf, &KdfParams::default_strong(), &[0; 16], &[0; 12]);
        buf[0] = b'X';
        assert_eq!(read_header(&buf).unwrap_err(), BackupError::Malformed);
    }

    #[test]
    fn header_rejects_unknown_version() {
        let mut buf = [0u8; HEADER_LEN];
        write_header(&mut buf, &KdfParams::default_strong(), &[0; 16], &[0; 12]);
        buf[5] = 0xFF;
        assert_eq!(
            read_header(&buf).unwrap_err(),
            BackupError::UnsupportedVersion
        );
    }
}
