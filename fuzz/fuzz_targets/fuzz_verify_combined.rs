//! Fuzz target: `signature::verify_combined` é robusto a inputs arbitrários
//! em sums, signature e hash hex. Garante que nenhum input causa panic e
//! que `Verified` nunca surge para bytes aleatórios (defesa contra
//! forjamento gratuito).

#![no_main]

use libfuzzer_sys::fuzz_target;
use mcpix_core::signature::{verify_combined, SignatureCheck, RELEASE_PUBKEY};

#[derive(arbitrary::Arbitrary, Debug)]
struct Input<'a> {
    sums: &'a [u8],
    sig: &'a [u8],
    filename: &'a str,
    hash_hex: &'a str,
}

fuzz_target!(|input: Input| {
    if let Ok(SignatureCheck::Verified) =
        verify_combined(input.sums, input.sig, RELEASE_PUBKEY, input.filename, input.hash_hex)
    {
        // Bytes aleatórios verificarem é forgery — panica deliberadamente para
        // o libfuzzer registrar como crash.
        panic!("random input produced SignatureCheck::Verified — possible forgery");
    }
});
