//! Helpers de manipulação segura de handles opacos atravessando FFI.
//!
//! Padrão: cada handle é um `Box::into_raw(Box::new(T))`. Vida útil é
//! controlada pela plataforma; a liberação acontece via função `*_free`
//! dedicada por tipo, que faz `Box::from_raw(ptr)` e descarta.

use std::panic::{catch_unwind, AssertUnwindSafe, UnwindSafe};

use crate::error::McpixStatus;

/// Embrulha qualquer closure FFI em `catch_unwind`, devolvendo `McpixStatus`
/// em caso de panic. Esta é a primeira linha da política de não-pânico
/// (Bloco 1.3 da spec).
pub(crate) fn guard<F>(f: F) -> McpixStatus
where
    F: FnOnce() -> McpixStatus + UnwindSafe,
{
    catch_unwind(f).unwrap_or(McpixStatus::Panic)
}

pub(crate) fn guard_mut<F>(f: F) -> McpixStatus
where
    F: FnOnce() -> McpixStatus,
{
    // `AssertUnwindSafe`: assumimos que o callable não deixa estado
    // observavelmente inconsistente em panic. Como todas as funções públicas
    // FFI seguem o padrão "computa local + commita no fim", isso vale.
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(s) => s,
        Err(_) => McpixStatus::Panic,
    }
}
