//! Implementações concretas da trait `Clock` do núcleo.
//!
//! - `SystemClock`: relógio do sistema (`std::time::SystemTime`). Default em
//!   produção, mas a trait fica injetada para permitir substituição em
//!   testes e em ambientes onde o relógio venha de TEE/HSM.
//! - `TestClock`: relógio determinístico controlado externamente. Útil para
//!   testes, demo reproduzível e cenários de drift simulado.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use mcpix_core::traits::Clock;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_secs(&self) -> u64 {
        // `SystemTime::now()` antes da época UNIX retorna Err. Tratamos isso
        // como 0 — é defensivo; clocks pré-1970 indicam configuração inválida
        // e o `TimestampQuantizedCounter` rejeita rollback abaixo do último T.
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// Relógio determinístico. Avança apenas quando o operador chama `set` ou
/// `advance`. Útil para testes que precisam controlar a janela quantizada.
pub struct TestClock {
    now: AtomicU64,
}

impl TestClock {
    pub fn new(initial_unix_secs: u64) -> Self {
        Self {
            now: AtomicU64::new(initial_unix_secs),
        }
    }

    pub fn set(&self, t: u64) {
        self.now.store(t, Ordering::SeqCst);
    }

    pub fn advance(&self, delta_secs: u64) {
        self.now.fetch_add(delta_secs, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now_unix_secs(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_positive_time() {
        // Sanity: relógio do sistema é > início do projeto.
        let now = SystemClock.now_unix_secs();
        assert!(now > 1_700_000_000, "system clock implausibly low: {now}");
    }

    #[test]
    fn test_clock_is_controllable() {
        let c = TestClock::new(1_000);
        assert_eq!(c.now_unix_secs(), 1_000);
        c.advance(30);
        assert_eq!(c.now_unix_secs(), 1_030);
        c.set(42);
        assert_eq!(c.now_unix_secs(), 42);
    }
}
