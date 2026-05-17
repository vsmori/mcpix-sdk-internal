//! Wrapper de `uniffi-bindgen`. Mantemos local ao workspace para garantir que
//! a versão do binário casa exatamente com a versão de `uniffi` usada por
//! `mcpix-uniffi` — UniFFI exige paridade de versão entre os dois lados.

fn main() {
    uniffi::uniffi_bindgen_main()
}
