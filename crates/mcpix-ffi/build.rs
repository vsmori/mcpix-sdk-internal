//! Build script — gera `bindings/c/include/mcpix.h` a partir do código.
//!
//! Estratégia: o header gerado é **commitado** ao repositório (a fim de que
//! consumidores .NET possam baixar o ZIP sem rodar Cargo). O job de CI chama
//! `cargo xtask gen-bindings` e diff-a o header — qualquer drift falha o build.

use std::env;
use std::path::PathBuf;

fn main() {
    // Não regenerar em modo `cargo publish` (sandbox sem write fora de OUT_DIR).
    if env::var_os("CARGO_PUBLISH").is_some() {
        return;
    }

    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_dir = crate_dir.parent().and_then(|p| p.parent()).unwrap();
    let header_out = workspace_dir.join("bindings/c/include/mcpix.h");

    if let Some(parent) = header_out.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let config = cbindgen::Config::from_file(crate_dir.join("cbindgen.toml"))
        .expect("cbindgen.toml not found or invalid");

    match cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
    {
        Ok(bindings) => {
            bindings.write_to_file(&header_out);
            println!("cargo:rerun-if-changed=src/");
            println!("cargo:rerun-if-changed=cbindgen.toml");
        }
        Err(e) => {
            // Não interromper o build do core por falha de cbindgen — apenas
            // avisar. A geração canônica acontece via `cargo xtask gen-bindings`.
            println!("cargo:warning=cbindgen skipped: {e}");
        }
    }
}
