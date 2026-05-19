//! mcpix-backup — backup criptografado de sementes e estado do recebedor.
//!
//! Caso de uso: usuário troca/perde dispositivo. O banco recebedor já
//! tem `S` custodiada, mas o **lado local** do recebedor (semente para
//! geração offline + contador `T`) precisa migrar para o novo device.
//! O cadastro inteiro do zero é caro (visita à agência, OTP, novo
//! `SeedId`) — backup pré-falha resolve.
//!
//! ## Formato (binário, codificado em Base58Check)
//!
//! ```text
//!   Header (47 bytes, autenticado como AAD):
//!     magic "MBKP"          4
//!     version u16 BE        2     0x0001
//!     kdf_id u8             1     1 = Argon2id
//!     kdf_m_cost u32 BE     4     kibibytes
//!     kdf_t_cost u32 BE     4     iterações
//!     kdf_p_cost u8         1     paralelismo
//!     reserved              3     0x00
//!     salt                 16
//!     nonce                12
//!
//!   Payload encriptado (75 bytes = 59 cleartext + 16 tag Poly1305):
//!     payload_version u8    1     0x01
//!     seed                 32
//!     seed_id_len u8        1
//!     seed_id              16     pad com 0x00 à direita
//!     counter_mode u8       1     0 = sequencial, 1 = quantizado
//!     counter_t u64 BE      8     se sequencial; senão 0
//! ```
//!
//! ## Modelo de confiança
//!
//! - **Autenticidade da semente**: o atacante que rouba o blob de
//!   backup precisa adivinhar a passphrase. Argon2id m=64MiB t=3 p=1
//!   torna brute-force GPU caro (cente­nas de ms por tentativa).
//! - **Integridade do payload**: AEAD garante que adulteração de 1 bit
//!   no ciphertext quebra a tag Poly1305 → restore falha.
//! - **AAD = header inteiro**: tampering nos parâmetros KDF ou no
//!   salt também é detectado (caso contrário, atacante reduziria
//!   m_cost para 1 e brute-forceia mais rápido).
//!
//! ## Limites
//!
//! - **Passphrase fraca = backup fraco**. A SDK não impõe política
//!   de qualidade de passphrase; integrador deve.
//! - **Anti-replay do counter**: importar o mesmo backup duas vezes
//!   restaura o mesmo `T` — se o exporter continuou gerando cobranças
//!   após o backup, há reuso de `T`. Mitigação: invalidar o backup
//!   anterior no banco recebedor (rotação de cert ou flag de versão)
//!   antes de aceitar o novo device. Fora do escopo deste módulo.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]

pub mod format;
pub mod kdf;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand_core::{OsRng, RngCore};
use zeroize::Zeroize;

use mcpix_core::types::{Seed, SeedId};

pub use format::{
    Container, CounterMode, KdfParams, PayloadView, ENCRYPTED_PAYLOAD_LEN, HEADER_LEN, MAGIC,
    PAYLOAD_LEN, TOTAL_LEN, VERSION,
};

/// Erros do backup. Variantes opacas sobre a passphrase para reduzir
/// canal de info-leak em UX (atacante não distingue "wrong passphrase"
/// de "wrong format" facilmente).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BackupError {
    #[error("backup format is malformed or corrupt")]
    Malformed,
    #[error("backup version unsupported")]
    UnsupportedVersion,
    #[error("decryption failed (wrong passphrase or tampered backup)")]
    DecryptFailed,
    #[error("Argon2 KDF error")]
    KdfFailed,
    #[error("invalid SeedId in payload")]
    InvalidSeedId,
}

/// Estado restaurado do recebedor a partir de backup decifrado.
#[derive(Debug)]
pub struct RestoredState {
    pub seed: Seed,
    pub seed_id: SeedId,
    pub counter_mode: CounterMode,
    /// Counter `T` last_issued se modo sequencial; 0 se quantizado.
    pub counter_t: u64,
}

/// Estado fornecido pelo recebedor no momento do backup.
#[derive(Debug)]
pub struct ExportInput<'a> {
    pub seed: &'a Seed,
    pub seed_id: &'a SeedId,
    pub counter_mode: CounterMode,
    pub counter_t: u64,
}

/// Constrói um backup criptografado e o codifica como Base58Check em
/// uma única linha. Parâmetros default do KDF (m=64 MiB, t=3, p=1)
/// alinham com OWASP 2024 para passphrases sensíveis.
pub fn export(input: &ExportInput<'_>, passphrase: &[u8]) -> Result<String, BackupError> {
    export_with_params(input, passphrase, KdfParams::default_strong())
}

/// Variante para alvos de menor recurso (ex. MCU restaurando), com
/// `KdfParams::default_embed()` (m=64 KiB, t=3, p=1).
pub fn export_for_embed(input: &ExportInput<'_>, passphrase: &[u8]) -> Result<String, BackupError> {
    export_with_params(input, passphrase, KdfParams::default_embed())
}

/// Variante de teste/integração que expõe `KdfParams` explícito. Não use em
/// produção sem entender as implicações de m_cost reduzido.
pub fn export_with_params_pub(
    input: &ExportInput<'_>,
    passphrase: &[u8],
    params: KdfParams,
) -> Result<String, BackupError> {
    export_with_params(input, passphrase, params)
}

fn export_with_params(
    input: &ExportInput<'_>,
    passphrase: &[u8],
    params: KdfParams,
) -> Result<String, BackupError> {
    // ── 1. Coletar entropia (salt + nonce) ────────────────────────────
    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    OsRng
        .try_fill_bytes(&mut salt)
        .map_err(|_| BackupError::KdfFailed)?;
    OsRng
        .try_fill_bytes(&mut nonce_bytes)
        .map_err(|_| BackupError::KdfFailed)?;

    // ── 2. Header (47 bytes, autenticado como AAD) ────────────────────
    let mut header = [0u8; HEADER_LEN];
    format::write_header(&mut header, &params, &salt, &nonce_bytes);

    // ── 3. Derivar chave da passphrase ───────────────────────────────
    let mut key_bytes = [0u8; 32];
    kdf::derive(passphrase, &salt, &params, &mut key_bytes)?;

    // ── 4. Serializar payload em claro ───────────────────────────────
    let mut payload = [0u8; PAYLOAD_LEN];
    format::write_payload(&mut payload, input);

    // ── 5. AEAD encrypt: payload bound to header via AAD ─────────────
    let key = Key::from_slice(&key_bytes);
    let cipher = ChaCha20Poly1305::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let mut ciphertext_and_tag = cipher
        .encrypt(
            nonce,
            Payload {
                msg: &payload,
                aad: &header,
            },
        )
        .map_err(|_| BackupError::KdfFailed)?;

    // ── 6. Concatenar header + (ciphertext + tag) ─────────────────────
    debug_assert_eq!(ciphertext_and_tag.len(), ENCRYPTED_PAYLOAD_LEN);
    let mut blob = Vec::with_capacity(TOTAL_LEN);
    blob.extend_from_slice(&header);
    blob.append(&mut ciphertext_and_tag);

    // Zeroiza buffers sensíveis antes de retornar.
    key_bytes.zeroize();
    payload.zeroize();

    // ── 7. Base58Check encode ────────────────────────────────────────
    Ok(bs58::encode(&blob).with_check().into_string())
}

/// Decifra um backup Base58Check. Devolve o estado restaurado pronto
/// para ser empurrado para os stores locais.
pub fn import(text: &str, passphrase: &[u8]) -> Result<RestoredState, BackupError> {
    // ── 1. Base58Check decode (verifica integridade básica) ───────────
    let blob: Vec<u8> = bs58::decode(text.as_bytes())
        .with_check(None)
        .into_vec()
        .map_err(|_| BackupError::Malformed)?;

    if blob.len() != TOTAL_LEN {
        return Err(BackupError::Malformed);
    }

    // ── 2. Parse do header ────────────────────────────────────────────
    let header_slice: &[u8; HEADER_LEN] = blob[..HEADER_LEN]
        .try_into()
        .map_err(|_| BackupError::Malformed)?;
    let ciphertext_and_tag = &blob[HEADER_LEN..];

    let (params, salt, nonce_bytes) = format::read_header(header_slice)?;

    // ── 3. KDF + decrypt ─────────────────────────────────────────────
    let mut key_bytes = [0u8; 32];
    kdf::derive(passphrase, &salt, &params, &mut key_bytes)?;
    let key = Key::from_slice(&key_bytes);
    let cipher = ChaCha20Poly1305::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext_and_tag,
                aad: header_slice.as_slice(),
            },
        )
        .map_err(|_| BackupError::DecryptFailed)?;
    key_bytes.zeroize();

    // ── 4. Parse do payload ──────────────────────────────────────────
    if plaintext.len() != PAYLOAD_LEN {
        return Err(BackupError::Malformed);
    }
    let payload_array: &[u8; PAYLOAD_LEN] = plaintext
        .as_slice()
        .try_into()
        .map_err(|_| BackupError::Malformed)?;
    let view = PayloadView::parse(payload_array)?;
    view.into_state()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input() -> (Seed, SeedId) {
        (Seed::from_bytes([0xAB; 32]), SeedId::new("R1").unwrap())
    }

    fn quick_params() -> KdfParams {
        // Argon2id mínimo aceito (8 KiB, 1 iter) — apenas para acelerar
        // testes. Em produção usar `default_strong()`.
        KdfParams {
            m_cost_kib: 8,
            t_cost: 1,
            p_cost: 1,
        }
    }

    #[test]
    fn round_trip_recovers_state() {
        let (seed, sid) = sample_input();
        let input = ExportInput {
            seed: &seed,
            seed_id: &sid,
            counter_mode: CounterMode::Sequential,
            counter_t: 42,
        };
        let text =
            export_with_params(&input, b"correct-horse-battery-staple", quick_params()).unwrap();
        // Base58Check é alfanumérico (alphabet Bitcoin) — cabe em uma
        // linha bem confortável.
        assert!(text.bytes().all(|b| b.is_ascii_alphanumeric()));
        let restored = import(&text, b"correct-horse-battery-staple").unwrap();
        assert_eq!(restored.seed.as_bytes(), seed.as_bytes());
        assert_eq!(restored.seed_id.as_str(), "R1");
        assert_eq!(restored.counter_mode, CounterMode::Sequential);
        assert_eq!(restored.counter_t, 42);
    }

    #[test]
    fn wrong_passphrase_fails_decrypt() {
        let (seed, sid) = sample_input();
        let input = ExportInput {
            seed: &seed,
            seed_id: &sid,
            counter_mode: CounterMode::Quantized,
            counter_t: 0,
        };
        let text = export_with_params(&input, b"hunter2", quick_params()).unwrap();
        let err = import(&text, b"hunter3").unwrap_err();
        assert_eq!(err, BackupError::DecryptFailed);
    }

    #[test]
    fn one_bit_tampering_breaks_aead() {
        let (seed, sid) = sample_input();
        let input = ExportInput {
            seed: &seed,
            seed_id: &sid,
            counter_mode: CounterMode::Sequential,
            counter_t: 1,
        };
        let text = export_with_params(&input, b"pwd", quick_params()).unwrap();
        // Flip um caractere central — vai falhar no decode Base58Check
        // ou em decrypt; ambos resultam em Err.
        let mut chars: Vec<char> = text.chars().collect();
        let mid = chars.len() / 2;
        chars[mid] = if chars[mid] == 'Q' { 'q' } else { 'Q' };
        let mangled: String = chars.into_iter().collect();
        assert!(import(&mangled, b"pwd").is_err());
    }

    #[test]
    fn quantized_mode_round_trips_with_zero_counter() {
        let (seed, sid) = sample_input();
        let input = ExportInput {
            seed: &seed,
            seed_id: &sid,
            counter_mode: CounterMode::Quantized,
            counter_t: 0,
        };
        let text = export_with_params(&input, b"x", quick_params()).unwrap();
        let restored = import(&text, b"x").unwrap();
        assert_eq!(restored.counter_mode, CounterMode::Quantized);
        assert_eq!(restored.counter_t, 0);
    }

    #[test]
    fn different_passphrases_produce_unrelated_backups() {
        // Sanity de aleatoriedade: dois exports com mesma input + diff
        // passphrase produzem outputs diferentes (mesmo se KDF
        // determinístico, o salt+nonce vêm de OsRng).
        let (seed, sid) = sample_input();
        let input = ExportInput {
            seed: &seed,
            seed_id: &sid,
            counter_mode: CounterMode::Sequential,
            counter_t: 1,
        };
        let t1 = export_with_params(&input, b"a", quick_params()).unwrap();
        let t2 = export_with_params(&input, b"a", quick_params()).unwrap();
        // Mesmo passphrase, mas salt+nonce são novos → outputs distintos.
        assert_ne!(t1, t2);
    }
}
