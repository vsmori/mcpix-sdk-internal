//! `SealedSeedStore` — `SeedStore` que persiste **apenas blobs selados**.
//!
//! A chave criptográfica de selagem vem do hardware seguro do
//! dispositivo (Secure Enclave / StrongBox / TPM), exposta como impl
//! da trait `mcpix_core::traits::SeedSealer`. A SDK provê
//! [`ChaChaSealer`] como **mock de referência** — usa chave em RAM,
//! adequada para testes e como skeleton de impls reais que precisam
//! plugar o material hw-bound.
//!
//! Veja `docs/SECURE_ELEMENT.md` para snippets de integração
//! iOS/Android/TPM. Estrutura interna do blob selado documentada em
//! [`ChaChaSealer`].
//!
//! ## O que esse módulo NÃO faz
//!
//! - **Não persiste** os blobs selados em disco. `SealedInMemorySeedStore`
//!   mantém um `HashMap` em RAM, mesmo que tipo do `InMemorySeedStore`.
//!   Integradores compõem o sealer com o backend de persistência que
//!   preferirem (SQLite, Keychain item, EEPROM no embed).
//! - **Não custodia receipts** (`save_receipt` / `get_receipt`). Os
//!   receipts (C₂ retido + flag `consumed`) não são segredos no mesmo
//!   sentido da Seed — viram opacos depois que `mark_consumed` rodou.
//!   Mantidos em claro aqui; quem quiser selar receipt-por-receipt
//!   estende o pattern.

use std::collections::HashMap;
use std::sync::Arc;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use parking_lot::RwLock;
use rand_core::{OsRng, RngCore};

use mcpix_core::error::McpixError;
use mcpix_core::traits::{SeedSealer, SeedStore};
use mcpix_core::types::{RetainedReceipt, Seed, SEED_LEN};

// ─────────────────────────────────────────────────────────────────────────────
// ChaChaSealer — impl de referência. ChaCha20-Poly1305 com nonce
// aleatório de 96 bits embutido no blob.
// ─────────────────────────────────────────────────────────────────────────────

const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
/// `nonce(12) || ciphertext(SEED_LEN) || tag(16)` = 60 bytes.
pub const SEALED_BLOB_LEN: usize = NONCE_LEN + SEED_LEN + TAG_LEN;

/// Sealer ChaCha20-Poly1305 com chave de 32 bytes em RAM.
///
/// **Aviso de segurança.** Esta impl é **mock de referência** — a
/// chave vive na heap do processo, sem proteção. Não use em produção
/// fora de teste/dev. Para fechar §7.1 a chave precisa vir do hardware
/// seguro (Secure Enclave / StrongBox / TPM); o snippet de integração
/// está em `docs/SECURE_ELEMENT.md`.
///
/// O blob produzido tem layout fixo:
/// ```text
///   ┌──────────┬───────────────────┬──────────┐
///   │ nonce 12 │ ciphertext 32     │ tag 16   │  total = 60 bytes
///   └──────────┴───────────────────┴──────────┘
/// ```
/// Não há AAD — a posição da Seed no store é dada por `SeedId`, que
/// também aparece em `RetainedReceipt`. AAD = `SeedId.as_bytes()`
/// blindaria contra "swap de blob entre slots" mas exigiria expor
/// `SeedId` no trait `SeedSealer`, complicando o contrato. Fica como
/// extensão se a feature `audit-trail` algum dia for ligada.
pub struct ChaChaSealer {
    cipher: ChaCha20Poly1305,
}

impl ChaChaSealer {
    /// Constrói com uma chave de 32 bytes. Em produção: chave nasce
    /// e morre no hardware seguro; aqui é o operador que passa.
    pub fn new(key: [u8; 32]) -> Self {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
        Self { cipher }
    }
}

impl SeedSealer for ChaChaSealer {
    fn seal(&self, plain: &Seed) -> Result<Vec<u8>, McpixError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng
            .try_fill_bytes(&mut nonce_bytes)
            .map_err(|e| McpixError::Storage(format!("seal: rng: {e}")))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct_and_tag = self
            .cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plain.as_bytes(),
                    aad: &[],
                },
            )
            .map_err(|_| McpixError::Storage("seal: aead encrypt failed".into()))?;
        debug_assert_eq!(ct_and_tag.len(), SEED_LEN + TAG_LEN);

        let mut blob = Vec::with_capacity(SEALED_BLOB_LEN);
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ct_and_tag);
        Ok(blob)
    }

    fn unseal(&self, blob: &[u8]) -> Result<Seed, McpixError> {
        if blob.len() != SEALED_BLOB_LEN {
            return Err(McpixError::Storage(format!(
                "unseal: bad blob length {} (expected {})",
                blob.len(),
                SEALED_BLOB_LEN
            )));
        }
        let nonce = Nonce::from_slice(&blob[..NONCE_LEN]);
        let pt = self
            .cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &blob[NONCE_LEN..],
                    aad: &[],
                },
            )
            .map_err(|_| {
                McpixError::Storage("unseal: aead decrypt failed (tampered or wrong key)".into())
            })?;
        if pt.len() != SEED_LEN {
            return Err(McpixError::Storage(format!(
                "unseal: bad plaintext length {}",
                pt.len()
            )));
        }
        let mut seed_bytes = [0u8; SEED_LEN];
        seed_bytes.copy_from_slice(&pt);
        Ok(Seed::from_bytes(seed_bytes))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SealedInMemorySeedStore — exemplo de SeedStore que **nunca** guarda
// a Seed em claro. Persistência em RAM; subclasses reais persistem em
// SQLite/Keychain/EEPROM.
// ─────────────────────────────────────────────────────────────────────────────

pub struct SealedInMemorySeedStore<W: SeedSealer> {
    sealer: Arc<W>,
    seeds: RwLock<HashMap<mcpix_core::types::SeedId, Vec<u8>>>,
    receipts: RwLock<HashMap<(mcpix_core::types::SeedId, u64), RetainedReceipt>>,
}

impl<W: SeedSealer> SealedInMemorySeedStore<W> {
    pub fn new(sealer: W) -> Self {
        Self {
            sealer: Arc::new(sealer),
            seeds: RwLock::new(HashMap::new()),
            receipts: RwLock::new(HashMap::new()),
        }
    }

    /// Acesso ao blob selado bruto — útil para inspeção em teste
    /// ou para integradores que queiram exportar o blob para outro
    /// store. Não exponha em produção sem cobrir com auditoria.
    pub fn peek_sealed_blob(&self, seed_id: &mcpix_core::types::SeedId) -> Option<Vec<u8>> {
        self.seeds.read().get(seed_id).cloned()
    }
}

impl<W: SeedSealer> SeedStore for SealedInMemorySeedStore<W> {
    fn put_seed(&self, seed_id: &mcpix_core::types::SeedId, seed: Seed) -> Result<(), McpixError> {
        let blob = self.sealer.seal(&seed)?;
        self.seeds.write().insert(seed_id.clone(), blob);
        // `seed` é dropada aqui — `ZeroizeOnDrop` apaga o material.
        Ok(())
    }

    fn get_seed(&self, seed_id: &mcpix_core::types::SeedId) -> Result<Option<Seed>, McpixError> {
        let blob = match self.seeds.read().get(seed_id) {
            Some(b) => b.clone(),
            None => return Ok(None),
        };
        Ok(Some(self.sealer.unseal(&blob)?))
    }

    fn save_receipt(&self, receipt: RetainedReceipt) -> Result<(), McpixError> {
        self.receipts
            .write()
            .insert((receipt.seed_id.clone(), receipt.counter), receipt);
        Ok(())
    }

    fn get_receipt(
        &self,
        seed_id: &mcpix_core::types::SeedId,
        counter: u64,
    ) -> Result<Option<RetainedReceipt>, McpixError> {
        Ok(self
            .receipts
            .read()
            .get(&(seed_id.clone(), counter))
            .cloned())
    }

    fn mark_consumed(
        &self,
        seed_id: &mcpix_core::types::SeedId,
        counter: u64,
    ) -> Result<(), McpixError> {
        let mut w = self.receipts.write();
        match w.get_mut(&(seed_id.clone(), counter)) {
            Some(r) => {
                r.consumed = true;
                Ok(())
            }
            None => Err(McpixError::NoRetainedReceipt),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monotonic_counter::InMemoryCounter;
    use crate::system_random::OsRandom;
    use crate::ReceiverSdk;
    use mcpix_core::state::ValidationOutcome;
    use mcpix_core::types::SeedId;

    fn sealer() -> ChaChaSealer {
        ChaChaSealer::new([0x77; 32])
    }

    // Invariante mais importante: o blob persistido **não contém** os
    // bytes da Seed em claro. Se este teste falhar, o sealer está
    // quebrado e §7.1 não foi de fato endereçado.
    #[test]
    fn sealed_blob_does_not_contain_plaintext_seed() {
        let store = SealedInMemorySeedStore::new(sealer());
        let sid = SeedId::new("R1").unwrap();
        let plain = [0xAB; SEED_LEN];
        store.put_seed(&sid, Seed::from_bytes(plain)).unwrap();
        let blob = store.peek_sealed_blob(&sid).unwrap();
        // Garantia mínima de tamanho.
        assert_eq!(blob.len(), SEALED_BLOB_LEN);
        // O slice 0xAB×32 não pode aparecer em lugar nenhum do blob.
        assert!(
            !blob.windows(SEED_LEN).any(|w| w == plain),
            "plaintext seed leaked into sealed blob"
        );
    }

    #[test]
    fn round_trip_through_store() {
        let store = SealedInMemorySeedStore::new(sealer());
        let sid = SeedId::new("R1").unwrap();
        let seed = Seed::from_bytes([0x42; 32]);
        store.put_seed(&sid, seed.clone()).unwrap();
        let got = store.get_seed(&sid).unwrap().unwrap();
        assert_eq!(got.as_bytes(), seed.as_bytes());
    }

    #[test]
    fn seal_is_not_deterministic() {
        // Cada `seal` usa nonce aleatório → blobs diferentes para
        // mesma Seed. Sem isso, dois slots com mesma Seed seriam
        // distinguíveis na inspeção do storage — vaza correlação.
        let s = sealer();
        let seed = Seed::from_bytes([0x11; 32]);
        let b1 = s.seal(&seed).unwrap();
        let b2 = s.seal(&seed).unwrap();
        assert_ne!(b1, b2);
    }

    #[test]
    fn tampered_blob_fails_decrypt() {
        let s = sealer();
        let seed = Seed::from_bytes([0x33; 32]);
        let mut blob = s.seal(&seed).unwrap();
        // Flip um bit no meio do ciphertext.
        blob[NONCE_LEN + 5] ^= 0x01;
        let err = s.unseal(&blob).unwrap_err();
        match err {
            McpixError::Storage(msg) => assert!(msg.contains("aead decrypt failed")),
            other => panic!("expected Storage error, got {other:?}"),
        }
    }

    #[test]
    fn wrong_sealer_fails_unseal() {
        // Cenário: dispositivo trocado, mas integrador esqueceu de
        // migrar a chave hw-bound. SealedSeedStore com chave nova
        // pega blob selado com chave antiga → falha clara, não
        // retorna lixo (que viraria forjamento silencioso).
        let s_old = ChaChaSealer::new([0x01; 32]);
        let s_new = ChaChaSealer::new([0x02; 32]);
        let seed = Seed::from_bytes([0x55; 32]);
        let blob = s_old.seal(&seed).unwrap();
        assert!(s_new.unseal(&blob).is_err());
    }

    #[test]
    fn full_charge_validation_through_sealed_store() {
        // Smoke ponta-a-ponta: o ReceiverSdk inteiro funciona
        // trocando InMemorySeedStore por SealedInMemorySeedStore.
        // Garante que o seal/unseal não muda o resultado funcional.
        let store: Arc<dyn SeedStore> = Arc::new(SealedInMemorySeedStore::new(sealer()));
        let counter = Arc::new(InMemoryCounter::new());
        let rng = Arc::new(OsRandom);
        let sdk = ReceiverSdk::new(store, counter, rng);

        let proof = sdk.register(SeedId::new("R1").unwrap()).unwrap();
        let charge = sdk.generate_charge(&proof.seed_id, 9900).unwrap();
        let retained = sdk
            .peek_retained(&proof.seed_id, charge.counter)
            .unwrap()
            .unwrap();
        let outcome = sdk
            .validate_receipt(
                &proof.seed_id,
                charge.counter,
                retained.expected_c2.as_str(),
            )
            .unwrap();
        assert_eq!(outcome, ValidationOutcome::Valid);
    }

    #[test]
    fn attestation_default_returns_none() {
        // Mock não publica atestação. Default da trait devolve None;
        // impls hw-bound real (SecureEnclave/StrongBox) substituem.
        assert_eq!(sealer().attestation().unwrap(), None);
    }
}
