//! Fuzz target: `signature::parse_sums_line` é robusto a entradas adversariais.
//!
//! Cenário: atacante injeta `SHA256SUMS` malformado para fazer o verificador
//! capotar antes mesmo de validar a assinatura.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mcpix_core::signature::parse_sums_line;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = core::str::from_utf8(data) {
        let _ = parse_sums_line(s);
    }
});
