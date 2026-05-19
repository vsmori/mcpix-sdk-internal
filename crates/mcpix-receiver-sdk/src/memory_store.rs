//! `SeedStore` em memória — implementação default para a demo.
//!
//! Substituição futura: a interface foi modelada para que uma impl em SQLite
//! (ver `sqlite_store` feature `sqlite`) ou um wrapper sobre Secure Enclave/TEE
//! caia sem mudanças no núcleo.

use std::collections::HashMap;

use parking_lot::Mutex;

use mcpix_core::error::McpixError;
use mcpix_core::traits::SeedStore;
use mcpix_core::types::{RetainedReceipt, Seed, SeedId};

#[derive(Default)]
pub struct InMemorySeedStore {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    seeds: HashMap<SeedId, Seed>,
    receipts: HashMap<(SeedId, u64), RetainedReceipt>,
}

impl InMemorySeedStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SeedStore for InMemorySeedStore {
    fn put_seed(&self, seed_id: &SeedId, seed: Seed) -> Result<(), McpixError> {
        self.inner.lock().seeds.insert(seed_id.clone(), seed);
        Ok(())
    }

    fn get_seed(&self, seed_id: &SeedId) -> Result<Option<Seed>, McpixError> {
        Ok(self.inner.lock().seeds.get(seed_id).cloned())
    }

    fn save_receipt(&self, receipt: RetainedReceipt) -> Result<(), McpixError> {
        let key = (receipt.seed_id.clone(), receipt.counter);
        self.inner.lock().receipts.insert(key, receipt);
        Ok(())
    }

    fn get_receipt(
        &self,
        seed_id: &SeedId,
        counter: u64,
    ) -> Result<Option<RetainedReceipt>, McpixError> {
        Ok(self
            .inner
            .lock()
            .receipts
            .get(&(seed_id.clone(), counter))
            .cloned())
    }

    fn mark_consumed(&self, seed_id: &SeedId, counter: u64) -> Result<(), McpixError> {
        let mut g = self.inner.lock();
        let key = (seed_id.clone(), counter);
        match g.receipts.get_mut(&key) {
            Some(r) => {
                r.consumed = true;
                Ok(())
            }
            None => Err(McpixError::NoRetainedReceipt),
        }
    }
}
