//! Fuzz target: `transport_field::parse` aceita qualquer bytes sem panic.
//!
//! Objetivo: garantir que o parser nunca capote o processo hospedeiro em
//! resposta a `txid` adversarial — viola a política de não-pânico do
//! Bloco 1.3 da especificação.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mcpix_core::transport_field::parse;

fuzz_target!(|data: &[u8]| {
    // Tenta como UTF-8; bytes inválidos são caso esperado de Err.
    if let Ok(s) = core::str::from_utf8(data) {
        let _ = parse(s);
    }
});
