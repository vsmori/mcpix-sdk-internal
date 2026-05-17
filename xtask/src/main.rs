//! cargo xtask — tarefas auxiliares do workspace.
//!
//! Uso:
//!   cargo xtask gen-bindings        regenera bindings/c, bindings/swift, bindings/kotlin
//!   cargo xtask check-bindings      regenera e falha se houver drift (uso em CI)
//!
//! Por que xtask: tarefas auxiliares como geração de bindings exigem múltiplas
//! invocações de cargo + scripts. Mantê-las em Rust evita shell scripts que
//! quebram entre Linux/macOS/Windows e elimina dependência externa de make.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p
}

fn cargo() -> Command {
    Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let cmd_str = format!("{cmd:?}");
    eprintln!("$ {cmd_str}");
    let status = cmd
        .status()
        .map_err(|e| format!("failed to spawn: {cmd_str}: {e}"))?;
    if !status.success() {
        return Err(format!("command failed ({status}): {cmd_str}"));
    }
    Ok(())
}

fn cdylib_path(root: &Path, crate_name: &str) -> PathBuf {
    let lib_name = crate_name.replace('-', "_");
    // Tentamos as três extensões; em Linux só `lib*.so` existe.
    for ext in ["so", "dylib", "dll"] {
        let p = root
            .join("target/debug")
            .join(format!("lib{lib_name}.{ext}"));
        if p.exists() {
            return p;
        }
        let p = root
            .join("target/debug")
            .join(format!("{lib_name}.{ext}"));
        if p.exists() {
            return p;
        }
    }
    root.join("target/debug")
        .join(format!("lib{lib_name}.so"))
}

fn gen_bindings(root: &Path) -> Result<(), String> {
    let swift_out = root.join("bindings/swift/Sources/MCPixSDK");
    let kotlin_out = root.join("bindings/kotlin/src/main/kotlin");
    let c_out = root.join("bindings/c/include");

    std::fs::create_dir_all(&swift_out).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&kotlin_out).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&c_out).map_err(|e| e.to_string())?;

    // 1) Compila os cdylib que UniFFI / cbindgen leem.
    run(cargo()
        .arg("build")
        .arg("-p")
        .arg("mcpix-uniffi")
        .arg("-p")
        .arg("mcpix-ffi"))?;

    let uniffi_lib = cdylib_path(root, "mcpix_uniffi");
    if !uniffi_lib.exists() {
        return Err(format!("uniffi cdylib not found: {}", uniffi_lib.display()));
    }

    // 2) Gera bindings Swift via uniffi-bindgen.
    run(cargo()
        .arg("run")
        .arg("-p")
        .arg("uniffi-bindgen")
        .arg("--")
        .arg("generate")
        .arg("--library")
        .arg(&uniffi_lib)
        .arg("--language")
        .arg("swift")
        .arg("--out-dir")
        .arg(&swift_out))?;

    // 3) Gera bindings Kotlin via uniffi-bindgen.
    run(cargo()
        .arg("run")
        .arg("-p")
        .arg("uniffi-bindgen")
        .arg("--")
        .arg("generate")
        .arg("--library")
        .arg(&uniffi_lib)
        .arg("--language")
        .arg("kotlin")
        .arg("--out-dir")
        .arg(&kotlin_out))?;

    // 4) Header C — cbindgen já roda no build.rs do mcpix-ffi. Aqui só
    //    forçamos rebuild para garantir refresh.
    run(cargo().arg("build").arg("-p").arg("mcpix-ffi"))?;

    eprintln!("bindings regenerados em {}", root.join("bindings").display());
    Ok(())
}

fn check_bindings(root: &Path) -> Result<(), String> {
    gen_bindings(root)?;
    let status = Command::new("git")
        .args(["diff", "--quiet", "--exit-code", "--", "bindings/"])
        .current_dir(root)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("bindings drift detected — run `cargo xtask gen-bindings` and commit".into());
    }
    Ok(())
}

fn print_help() {
    println!("xtask — workspace helper tasks\n");
    println!("USAGE:");
    println!("  cargo xtask <COMMAND>\n");
    println!("COMMANDS:");
    println!("  gen-bindings     Regenerate bindings/c, bindings/swift, bindings/kotlin");
    println!("  check-bindings   Regenerate and fail if there is git drift");
}

fn main() -> ExitCode {
    let cmd = env::args().nth(1);
    let root = workspace_root();
    let result = match cmd.as_deref() {
        Some("gen-bindings") => gen_bindings(&root),
        Some("check-bindings") => check_bindings(&root),
        Some("--help") | Some("-h") | None => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Some(other) => Err(format!("unknown command: {other}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
