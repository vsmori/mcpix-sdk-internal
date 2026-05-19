//! Wrapper sobre Argon2id para derivar chave AEAD da passphrase.
//!
//! API minimalista: dado `passphrase`, `salt` e `KdfParams`, escreve
//! 32 bytes em `out` (chave de ChaCha20-Poly1305).

use argon2::{Algorithm, Argon2, Params, Version};

use crate::format::KdfParams;
use crate::BackupError;

pub fn derive(
    passphrase: &[u8],
    salt: &[u8; 16],
    params: &KdfParams,
    out: &mut [u8; 32],
) -> Result<(), BackupError> {
    let argon_params = Params::new(
        params.m_cost_kib,
        params.t_cost,
        params.p_cost as u32,
        Some(32),
    )
    .map_err(|_| BackupError::KdfFailed)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    argon
        .hash_password_into(passphrase, salt, out)
        .map_err(|_| BackupError::KdfFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_is_deterministic_for_same_inputs() {
        let salt = [0x11u8; 16];
        let params = KdfParams {
            m_cost_kib: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let mut k1 = [0u8; 32];
        let mut k2 = [0u8; 32];
        derive(b"hunter2", &salt, &params, &mut k1).unwrap();
        derive(b"hunter2", &salt, &params, &mut k2).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn different_salt_produces_different_keys() {
        let params = KdfParams {
            m_cost_kib: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let mut k1 = [0u8; 32];
        let mut k2 = [0u8; 32];
        derive(b"x", &[0xAA; 16], &params, &mut k1).unwrap();
        derive(b"x", &[0xBB; 16], &params, &mut k2).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn different_passphrase_produces_different_keys() {
        let salt = [0x11u8; 16];
        let params = KdfParams {
            m_cost_kib: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let mut k1 = [0u8; 32];
        let mut k2 = [0u8; 32];
        derive(b"alpha", &salt, &params, &mut k1).unwrap();
        derive(b"beta", &salt, &params, &mut k2).unwrap();
        assert_ne!(k1, k2);
    }
}
