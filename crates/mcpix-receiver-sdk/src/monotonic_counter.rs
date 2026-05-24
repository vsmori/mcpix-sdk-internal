use std::collections::HashMap;

use parking_lot::Mutex;

use mcpix_core::error::McpixError;
use mcpix_core::traits::Counter;
use mcpix_core::types::SeedId;

#[derive(Default)]
pub struct InMemoryCounter {
    inner: Mutex<HashMap<SeedId, u64>>,
}

impl InMemoryCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Semeia o "último T emitido" para `seed_id` — usado ao restaurar
    /// de um backup (mcpix-backup) em modo sequencial. A próxima
    /// chamada a `next()` devolverá `last_t + 1`, preservando a
    /// monotonia mesmo após troca de dispositivo.
    ///
    /// Idempotente em relação ao maior valor: se o slot já tem um T
    /// maior (e.g. duas restaurações), mantém o maior — nunca anda
    /// para trás, que seria reuso de T.
    pub fn restore_last_issued(&self, seed_id: &SeedId, last_t: u64) {
        let mut g = self.inner.lock();
        let slot = g.entry(seed_id.clone()).or_insert(0);
        if last_t > *slot {
            *slot = last_t;
        }
    }
}

impl Counter for InMemoryCounter {
    fn next(&self, seed_id: &SeedId) -> Result<u64, McpixError> {
        let mut g = self.inner.lock();
        let slot = g.entry(seed_id.clone()).or_insert(0);
        let next = slot.checked_add(1).ok_or(McpixError::CounterOverflow)?;
        *slot = next;
        Ok(next)
    }
}
