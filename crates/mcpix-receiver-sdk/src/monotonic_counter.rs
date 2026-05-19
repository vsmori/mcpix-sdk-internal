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
