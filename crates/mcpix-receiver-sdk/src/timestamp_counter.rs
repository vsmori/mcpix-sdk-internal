//! Contador derivado de **timestamp quantizado** — modo alternativo ao
//! `InMemoryCounter` sequencial.
//!
//! Fundamento: a especificação técnica admite `T` como (i) contador unidirecional
//! sequencial ou (ii) timestamp quantizado. Esta impl é a forma (ii).
//!
//! `T = now_unix_secs / window_seconds`
//!
//! Janela padrão: 30 segundos (consonante com RFC 6238 TOTP). Configurável.
//!
//! ## Garantias enforçadas
//!
//! 1. **Monotonia**: chamadas com `t_now > t_anterior` para o mesmo `SeedId`
//!    avançam o contador.
//! 2. **Anti-colisão**: chamadas dentro do mesmo quantum retornam
//!    `McpixError::CounterCollision`. Sem isso, duas cobranças no mesmo
//!    window produziriam o mesmo C₁ e a segunda sobrescreveria o retained
//!    receipt da primeira — silenciosamente invalidando a primeira.
//! 3. **Anti-rollback**: relógio que recua para `t < t_anterior` retorna
//!    `McpixError::CounterRollback`. Defende contra adversário que ajusta o
//!    relógio do dispositivo para reusar um T antigo.
//!
//! ## Drift entre recebedor e banco do pagador
//!
//! Ambos os lados quantizam o **mesmo** wall clock para a **mesma** janela.
//! Se os clocks divergirem em mais que `window_seconds`, derivam T distintos
//! → C₂ distintos → `Mismatch`. A tolerância de ±1 window é responsabilidade
//! do bank-payer-mock (ver `recover_c2_with_tolerance` lá).

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use mcpix_core::error::McpixError;
use mcpix_core::traits::{Clock, Counter};
use mcpix_core::types::SeedId;

pub const DEFAULT_WINDOW_SECONDS: u64 = 30;

pub struct TimestampQuantizedCounter {
    window_seconds: u64,
    clock: Arc<dyn Clock>,
    last_issued: Mutex<HashMap<SeedId, u64>>,
}

impl TimestampQuantizedCounter {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self::with_window(clock, DEFAULT_WINDOW_SECONDS)
    }

    pub fn with_window(clock: Arc<dyn Clock>, window_seconds: u64) -> Self {
        assert!(window_seconds > 0, "window_seconds must be > 0");
        Self {
            window_seconds,
            clock,
            last_issued: Mutex::new(HashMap::new()),
        }
    }

    pub fn window_seconds(&self) -> u64 {
        self.window_seconds
    }

    /// Computa o quantum atual sem mutar estado. Útil para o bank-payer-mock
    /// derivar o `counter` esperado a partir do clock local.
    pub fn current_quantum(&self) -> u64 {
        self.clock.now_unix_secs() / self.window_seconds
    }
}

impl Counter for TimestampQuantizedCounter {
    fn next(&self, seed_id: &SeedId) -> Result<u64, McpixError> {
        let now = self.clock.now_unix_secs();
        let t = now / self.window_seconds;

        let mut g = self.last_issued.lock();
        match g.get(seed_id).copied() {
            None => {
                g.insert(seed_id.clone(), t);
                Ok(t)
            }
            Some(last) if t > last => {
                g.insert(seed_id.clone(), t);
                Ok(t)
            }
            Some(last) if t == last => Err(McpixError::CounterCollision {
                window_seconds: self.window_seconds,
            }),
            Some(last) => Err(McpixError::CounterRollback { last, now: t }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::TestClock;

    fn sid() -> SeedId {
        SeedId::new("R1").unwrap()
    }

    #[test]
    fn first_call_uses_quantized_now() {
        let clk = Arc::new(TestClock::new(1_000_000));
        let c = TimestampQuantizedCounter::with_window(clk, 30);
        let t = c.next(&sid()).unwrap();
        assert_eq!(t, 1_000_000 / 30);
    }

    #[test]
    fn same_window_call_is_rejected() {
        let clk = Arc::new(TestClock::new(1_000_000));
        let c = TimestampQuantizedCounter::with_window(clk.clone(), 30);
        c.next(&sid()).unwrap();
        // Avança apenas 5s — ainda dentro do mesmo quantum.
        clk.advance(5);
        let err = c.next(&sid()).unwrap_err();
        assert!(matches!(
            err,
            McpixError::CounterCollision { window_seconds: 30 }
        ));
    }

    #[test]
    fn cross_window_call_succeeds() {
        let clk = Arc::new(TestClock::new(1_000_000));
        let c = TimestampQuantizedCounter::with_window(clk.clone(), 30);
        let t1 = c.next(&sid()).unwrap();
        clk.advance(30);
        let t2 = c.next(&sid()).unwrap();
        assert_eq!(t2, t1 + 1);
    }

    #[test]
    fn clock_rollback_is_rejected() {
        let clk = Arc::new(TestClock::new(1_000_000));
        let c = TimestampQuantizedCounter::with_window(clk.clone(), 30);
        c.next(&sid()).unwrap();
        // Retrocede o relógio em 1 quantum cheio.
        clk.set(1_000_000 - 60);
        let err = c.next(&sid()).unwrap_err();
        assert!(matches!(err, McpixError::CounterRollback { .. }));
    }

    #[test]
    fn isolation_between_seed_ids() {
        let clk = Arc::new(TestClock::new(1_000_000));
        let c = TimestampQuantizedCounter::with_window(clk, 30);
        c.next(&SeedId::new("R1").unwrap()).unwrap();
        // Outro recebedor pode emitir no mesmo quantum.
        c.next(&SeedId::new("R2").unwrap()).unwrap();
    }

    #[test]
    fn current_quantum_does_not_mutate() {
        let clk = Arc::new(TestClock::new(1_000_000));
        let c = TimestampQuantizedCounter::with_window(clk, 30);
        let q1 = c.current_quantum();
        let q2 = c.current_quantum();
        assert_eq!(q1, q2);
        // E ainda permite next() depois.
        let _ = c.next(&sid()).unwrap();
    }
}
