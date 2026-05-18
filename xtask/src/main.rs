//! cargo xtask — tarefas auxiliares do workspace.
//!
//! Uso:
//!   cargo xtask gen-bindings           regenera bindings/c, bindings/swift, bindings/kotlin
//!   cargo xtask check-bindings         regenera e falha se houver drift
//!   cargo xtask build-linux            x86_64-unknown-linux-gnu (release)
//!   cargo xtask build-windows          x86_64-pc-windows-gnu (release; requer mingw-w64)
//!   cargo xtask build-android          4 ABIs Android via cargo-ndk (requer ANDROID_NDK_HOME)
//!   cargo xtask build-ios              aarch64-apple-ios + aarch64-apple-ios-sim (somente macOS)
//!   cargo xtask package-aar            empacota libmcpix_uniffi.so em AAR via gradle :assemble
//!   cargo xtask package-xcframework    empacota .a iOS em XCFramework (somente macOS)
//!   cargo xtask package-nuget          gera .nupkg com .dll/.so via dotnet pack
//!   cargo xtask build-all              tudo aplicável ao host atual
//!   cargo xtask hash-artifacts         escreve dist/SHA256SUMS
//!   cargo xtask gen-release-key        gera novo par Ed25519 (pub commitada, priv escrita
//!                                      em arquivo + impressa). USAR APENAS UMA VEZ por rotação.
//!   cargo xtask sign-artifacts         assina dist/SHA256SUMS com MCPIX_SIGN_PRIVKEY_HEX
//!                                      (env var contendo 64 chars hex = 32 bytes seed)
//!
//! Saída padronizada em `dist/<plataforma>/`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p
}

fn dist_dir() -> PathBuf {
    workspace_root().join("dist")
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

fn mkdir(p: &Path) -> Result<(), String> {
    fs::create_dir_all(p).map_err(|e| format!("mkdir {}: {e}", p.display()))
}

fn copy_into(src: &Path, dst_dir: &Path) -> Result<PathBuf, String> {
    mkdir(dst_dir)?;
    let dst = dst_dir.join(src.file_name().unwrap());
    fs::copy(src, &dst).map_err(|e| format!("copy {} -> {}: {e}", src.display(), dst.display()))?;
    Ok(dst)
}

fn target_dir(target: &str, profile: &str) -> PathBuf {
    workspace_root()
        .join("target")
        .join(target)
        .join(profile)
}

// ─────────────────────────────────────────────────────────────────────────────
// gen-bindings / check-bindings (sessão 2)
// ─────────────────────────────────────────────────────────────────────────────

fn cdylib_path(root: &Path, crate_name: &str) -> PathBuf {
    let lib_name = crate_name.replace('-', "_");
    for ext in ["so", "dylib", "dll"] {
        let p = root.join("target/debug").join(format!("lib{lib_name}.{ext}"));
        if p.exists() {
            return p;
        }
        let p = root.join("target/debug").join(format!("{lib_name}.{ext}"));
        if p.exists() {
            return p;
        }
    }
    root.join("target/debug").join(format!("lib{lib_name}.so"))
}

fn gen_bindings(root: &Path) -> Result<(), String> {
    let swift_out = root.join("bindings/swift/Sources/MCPixSDK");
    let kotlin_out = root.join("bindings/kotlin/src/main/kotlin");
    let c_out = root.join("bindings/c/include");
    mkdir(&swift_out)?;
    mkdir(&kotlin_out)?;
    mkdir(&c_out)?;

    run(cargo()
        .arg("build")
        .arg("-p").arg("mcpix-uniffi")
        .arg("-p").arg("mcpix-ffi"))?;

    let uniffi_lib = cdylib_path(root, "mcpix_uniffi");
    if !uniffi_lib.exists() {
        return Err(format!("uniffi cdylib not found: {}", uniffi_lib.display()));
    }
    for lang in ["swift", "kotlin"] {
        let out = if lang == "swift" { &swift_out } else { &kotlin_out };
        run(cargo()
            .arg("run").arg("-p").arg("uniffi-bindgen").arg("--")
            .arg("generate")
            .arg("--library").arg(&uniffi_lib)
            .arg("--language").arg(lang)
            .arg("--out-dir").arg(out))?;
    }
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

// ─────────────────────────────────────────────────────────────────────────────
// Cross-compile + empacotamento (sessão 3)
// ─────────────────────────────────────────────────────────────────────────────

/// Build de release para um target. Aceita variável de ambiente
/// `MCPIX_EXPECTED_SHA256` que será injetada no binário para o self-check.
fn build_target(target: &str, packages: &[&str]) -> Result<(), String> {
    let mut cmd = cargo();
    cmd.arg("build").arg("--release").arg("--target").arg(target);
    for p in packages {
        cmd.arg("-p").arg(p);
    }
    run(&mut cmd)
}

fn build_linux(root: &Path) -> Result<(), String> {
    let target = "x86_64-unknown-linux-gnu";
    build_target(target, &["mcpix-uniffi", "mcpix-ffi"])?;
    let out = dist_dir().join("linux-x86_64");
    mkdir(&out)?;
    let src = target_dir(target, "release");
    copy_into(&src.join("libmcpix_uniffi.so"), &out)?;
    copy_into(&src.join("libmcpix_ffi.so"), &out)?;
    copy_into(&root.join("bindings/c/include/mcpix.h"), &out)?;
    Ok(())
}

fn build_windows(root: &Path) -> Result<(), String> {
    let target = "x86_64-pc-windows-gnu";
    build_target(target, &["mcpix-uniffi", "mcpix-ffi"])?;
    let out = dist_dir().join("windows-x86_64");
    mkdir(&out)?;
    let src = target_dir(target, "release");
    copy_into(&src.join("mcpix_uniffi.dll"), &out)?;
    copy_into(&src.join("mcpix_ffi.dll"), &out)?;
    copy_into(&root.join("bindings/c/include/mcpix.h"), &out)?;
    Ok(())
}

const ANDROID_ABIS: &[(&str, &str)] = &[
    ("arm64-v8a", "aarch64-linux-android"),
    ("armeabi-v7a", "armv7-linux-androideabi"),
    ("x86_64", "x86_64-linux-android"),
    ("x86", "i686-linux-android"),
];

fn build_android(root: &Path) -> Result<(), String> {
    if env::var("ANDROID_NDK_HOME").is_err() {
        return Err("ANDROID_NDK_HOME unset — install Android NDK r25+ and export the path".into());
    }
    // cargo-ndk lida com linkers/sysroot Android automaticamente, baseado no
    // ANDROID_NDK_HOME. Passamos os 4 ABIs em uma única invocação.
    let mut cmd = cargo();
    cmd.arg("ndk");
    for (abi, _) in ANDROID_ABIS {
        cmd.arg("-t").arg(abi);
    }
    cmd.arg("-o").arg(dist_dir().join("android/jniLibs"));
    cmd.arg("--").arg("build").arg("--release")
        .arg("-p").arg("mcpix-uniffi");
    run(&mut cmd)?;
    // Header C também vai para o pacote AAR (para integradores nativos).
    let out = dist_dir().join("android");
    copy_into(&root.join("bindings/c/include/mcpix.h"), &out.join("include"))?;
    Ok(())
}

const IOS_TARGETS: &[(&str, &str)] = &[
    ("device", "aarch64-apple-ios"),
    ("simulator-arm64", "aarch64-apple-ios-sim"),
    ("simulator-x86_64", "x86_64-apple-ios"),
];

fn build_ios(_root: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("iOS build requires macOS host (lipo + xcodebuild)".into());
    }
    for (_, target) in IOS_TARGETS {
        build_target(target, &["mcpix-uniffi"])?;
    }
    Ok(())
}

fn package_xcframework(root: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("XCFramework packaging requires macOS (xcodebuild)".into());
    }
    let out = dist_dir().join("ios");
    mkdir(&out)?;

    let device_a = target_dir("aarch64-apple-ios", "release").join("libmcpix_uniffi.a");
    let sim_arm_a = target_dir("aarch64-apple-ios-sim", "release").join("libmcpix_uniffi.a");
    let sim_x86_a = target_dir("x86_64-apple-ios", "release").join("libmcpix_uniffi.a");

    // lipo das duas variantes do simulador num único `.a` fat.
    let sim_combined = out.join("libmcpix_uniffi-sim.a");
    run(Command::new("lipo")
        .arg("-create")
        .arg(&sim_arm_a).arg(&sim_x86_a)
        .arg("-output").arg(&sim_combined))?;

    let headers = root.join("bindings/swift/Sources/MCPixSDK");
    let xcfwk = out.join("MCPixSDKFFI.xcframework");
    if xcfwk.exists() {
        fs::remove_dir_all(&xcfwk).ok();
    }
    run(Command::new("xcodebuild")
        .arg("-create-xcframework")
        .arg("-library").arg(&device_a).arg("-headers").arg(&headers)
        .arg("-library").arg(&sim_combined).arg("-headers").arg(&headers)
        .arg("-output").arg(&xcfwk))?;
    Ok(())
}

fn package_aar(root: &Path) -> Result<(), String> {
    // Gradle :aar:assembleRelease empacota jniLibs já organizadas em
    // `dist/android/jniLibs/<abi>/libmcpix_uniffi.so` (saída do build_android).
    let aar_proj = root.join("bindings/kotlin");
    let dist_aar_libs = dist_dir().join("android/jniLibs");
    if !dist_aar_libs.exists() {
        return Err(format!(
            "expected jniLibs at {} — run `cargo xtask build-android` first",
            dist_aar_libs.display()
        ));
    }
    // Variável que o build.gradle.kts do AAR usa para encontrar os .so.
    let status = Command::new("gradle")
        .arg(":aar:assembleRelease")
        .arg("--no-daemon")
        .arg(format!("-Pjnilibs.dir={}", dist_aar_libs.display()))
        .current_dir(&aar_proj)
        .status()
        .map_err(|e| format!("gradle: {e}"))?;
    if !status.success() {
        return Err(format!("gradle assembleRelease failed: {status}"));
    }
    // Copia o AAR final para dist/.
    let built = aar_proj.join("aar/build/outputs/aar/aar-release.aar");
    if built.exists() {
        copy_into(&built, &dist_dir().join("android"))?;
    }
    Ok(())
}

fn package_nuget(root: &Path) -> Result<(), String> {
    let csproj = root.join("bindings/dotnet/MCPixSDK.csproj");
    if !csproj.exists() {
        return Err(format!("missing {}", csproj.display()));
    }
    run(Command::new("dotnet")
        .arg("pack")
        .arg(&csproj)
        .arg("-c").arg("Release")
        .arg("-o").arg(dist_dir().join("nuget")))?;
    Ok(())
}

fn build_all(root: &Path) -> Result<(), String> {
    // Sempre faz Linux. Demais alvos: tenta e ignora "host não suporta".
    build_linux(root)?;
    let _ = build_windows(root).map_err(|e| eprintln!("warning: windows skipped: {e}"));
    let _ = build_android(root).map_err(|e| eprintln!("warning: android skipped: {e}"));
    if cfg!(target_os = "macos") {
        let _ = build_ios(root).map_err(|e| eprintln!("warning: ios skipped: {e}"));
        let _ = package_xcframework(root).map_err(|e| eprintln!("warning: xcframework skipped: {e}"));
    }
    Ok(())
}

/// Escreve `dist/SHA256SUMS` no formato `sha256sum -c` compatível, cobrindo
/// todos os arquivos sob `dist/` recursivamente. Saída usada por CI para
/// publicação dos hashes esperados e para alimentar `MCPIX_EXPECTED_SHA256`
/// do próximo build (auto-binding do self-check).
fn hash_artifacts(_root: &Path) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let dist = dist_dir();
    if !dist.exists() {
        return Err("no dist/ — run `cargo xtask build-all` first".into());
    }
    let mut lines: Vec<String> = Vec::new();
    walk(&dist, &dist, &mut |path: &Path, rel: &Path| {
        if rel.file_name().and_then(|s| s.to_str()) == Some("SHA256SUMS") {
            return Ok(());
        }
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest = hasher.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        lines.push(format!("{hex}  {}", rel.display()));
        Ok(())
    })?;
    lines.sort();
    fs::write(dist.join("SHA256SUMS"), lines.join("\n") + "\n")
        .map_err(|e| e.to_string())?;
    eprintln!("dist/SHA256SUMS atualizado ({} entradas)", lines.len());
    Ok(())
}

fn walk(
    base: &Path,
    p: &Path,
    f: &mut dyn FnMut(&Path, &Path) -> Result<(), String>,
) -> Result<(), String> {
    let entries = fs::read_dir(p).map_err(|e| format!("read_dir {}: {e}", p.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            walk(base, &path, f)?;
        } else {
            let rel = path.strip_prefix(base).unwrap();
            f(&path, rel)?;
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Assinatura (sessão 4)
// ─────────────────────────────────────────────────────────────────────────────

fn gen_release_key(root: &Path) -> Result<(), String> {
    use ed25519_dalek::{SigningKey, SECRET_KEY_LENGTH};
    use rand_core::OsRng;

    let pub_path = root.join("crates/mcpix-core/trusted_keys/release.pub");
    let priv_path = root.join("target/release-key.priv");
    mkdir(pub_path.parent().unwrap())?;
    mkdir(priv_path.parent().unwrap())?;

    if pub_path.exists() {
        return Err(format!(
            "release.pub already exists at {} — refusing to overwrite. \
             Rotation must be explicit: delete the file first and re-run.",
            pub_path.display()
        ));
    }

    let sk = SigningKey::generate(&mut OsRng);
    let pk = sk.verifying_key();
    let seed: [u8; SECRET_KEY_LENGTH] = sk.to_bytes();

    fs::write(&pub_path, pk.to_bytes()).map_err(|e| e.to_string())?;
    fs::write(&priv_path, seed).map_err(|e| e.to_string())?;

    let priv_hex: String = seed.iter().map(|b| format!("{b:02x}")).collect();
    eprintln!("wrote {}", pub_path.display());
    eprintln!("wrote {} (do NOT commit)", priv_path.display());
    eprintln!();
    eprintln!("Private key (hex, set as MCPIX_SIGN_PRIVKEY_HEX in CI secrets):");
    eprintln!("  {priv_hex}");
    eprintln!();
    eprintln!("Public key (hex, for cross-checking):");
    let pub_hex: String = pk.to_bytes().iter().map(|b| format!("{b:02x}")).collect();
    eprintln!("  {pub_hex}");
    Ok(())
}

fn sign_artifacts(_root: &Path) -> Result<(), String> {
    use ed25519_dalek::{Signer, SigningKey};

    let sums = dist_dir().join("SHA256SUMS");
    if !sums.exists() {
        return Err(format!(
            "{} not found — run `cargo xtask hash-artifacts` first",
            sums.display()
        ));
    }
    let key_hex = env::var("MCPIX_SIGN_PRIVKEY_HEX").map_err(|_| {
        "MCPIX_SIGN_PRIVKEY_HEX env var unset — pass the 64-char hex seed of the \
         release private key (in CI: from secret)".to_string()
    })?;
    if key_hex.len() != 64 {
        return Err(format!(
            "MCPIX_SIGN_PRIVKEY_HEX must be 64 hex chars, got {}",
            key_hex.len()
        ));
    }
    let mut seed = [0u8; 32];
    for (i, chunk) in key_hex.as_bytes().chunks(2).enumerate() {
        let s = core::str::from_utf8(chunk).map_err(|e| e.to_string())?;
        seed[i] = u8::from_str_radix(s, 16).map_err(|e| e.to_string())?;
    }
    let sk = SigningKey::from_bytes(&seed);

    let bytes = fs::read(&sums).map_err(|e| e.to_string())?;
    let sig = sk.sign(&bytes).to_bytes();
    let sig_path = dist_dir().join("SHA256SUMS.sig");
    fs::write(&sig_path, sig).map_err(|e| e.to_string())?;
    eprintln!("wrote {} ({} bytes)", sig_path.display(), sig.len());
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Dispatcher
// ─────────────────────────────────────────────────────────────────────────────

fn print_help() {
    println!("xtask — workspace helper tasks\n");
    println!("USAGE:");
    println!("  cargo xtask <COMMAND>\n");
    println!("COMMANDS:");
    println!("  gen-bindings         Regenerate bindings/c, bindings/swift, bindings/kotlin");
    println!("  check-bindings       Regenerate and fail if there is git drift");
    println!("  build-linux          Release build for x86_64-unknown-linux-gnu → dist/linux-x86_64");
    println!("  build-windows        Release build for x86_64-pc-windows-gnu → dist/windows-x86_64");
    println!("  build-android        4-ABI Android via cargo-ndk → dist/android/jniLibs");
    println!("  build-ios            iOS device + simulator (macOS only) → target/aarch64-apple-ios/...");
    println!("  package-aar          Bundle jniLibs into an .aar via Gradle");
    println!("  package-xcframework  Bundle iOS .a into MCPixSDKFFI.xcframework (macOS only)");
    println!("  package-nuget        Pack .nupkg via dotnet pack");
    println!("  build-all            Run every step available on the current host");
    println!("  hash-artifacts       Write dist/SHA256SUMS over every file in dist/");
    println!("  gen-release-key      Generate a new Ed25519 release keypair");
    println!("  sign-artifacts       Sign dist/SHA256SUMS with MCPIX_SIGN_PRIVKEY_HEX");
    println!("  build-wasm           Build mcpix-wasm + run wasm-bindgen → examples/web-demo/pkg");
    println!("  fuzz-replay          Replay versioned corpus through fuzz targets (no nightly)");
    println!("  verify-hermetic      Vendor deps and build with --frozen --locked --offline (SLSA L4 prep)");
}

fn verify_hermetic(root: &Path) -> Result<(), String> {
    // Esta task implementa a **primeira fase** de SLSA L4: prova
    // operacional de que o build não depende de rede após o ponto de
    // vendor. Sequência:
    //   1) `cargo vendor` baixa todas as deps para `./vendor/`.
    //   2) Escreve `.cargo/config.hermetic.toml` apontando o source
    //      de crates.io para o diretório vendored.
    //   3) `cargo build --frozen --locked --offline` com a config
    //      acima — se algum dep faltar ou cargo tentar resolver da
    //      rede, falha.
    //
    // A *segunda* fase (cross-runner hash compare para reprodutibilidade
    // bit-exata) fica como roadmap — exige run pareado em CI.
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());

    println!("[1/3] cargo vendor (esta etapa baixa toda a árvore de deps)");
    let vendor_config_path = root.join(".cargo/config.hermetic.toml");
    std::fs::create_dir_all(root.join(".cargo"))
        .map_err(|e| format!("mkdir .cargo: {e}"))?;
    let output = std::process::Command::new(&cargo)
        .args(["vendor", "--locked", "vendor"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo vendor failed to spawn: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo vendor failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    std::fs::write(&vendor_config_path, &output.stdout)
        .map_err(|e| format!("write {}: {e}", vendor_config_path.display()))?;
    println!("    config escrita em {}", vendor_config_path.display());

    println!("[2/3] cargo build --frozen --locked --offline (workspace default features)");
    let status = std::process::Command::new(&cargo)
        .args([
            "build",
            "--workspace",
            "--frozen",
            "--locked",
            "--offline",
            "--config",
            &format!("include=[\"{}\"]", vendor_config_path.display()),
        ])
        .current_dir(root)
        .status()
        .map_err(|e| format!("cargo build hermetic failed to spawn: {e}"))?;
    if !status.success() {
        return Err(
            "cargo build hermetic falhou — alguma dep tentou rede ou está faltando do vendor"
                .into(),
        );
    }

    println!("[3/3] mesmo build com --all-targets (testes, exemplos, benches)");
    let status = std::process::Command::new(&cargo)
        .args([
            "build",
            "--workspace",
            "--all-targets",
            "--frozen",
            "--locked",
            "--offline",
            "--config",
            &format!("include=[\"{}\"]", vendor_config_path.display()),
        ])
        .current_dir(root)
        .status()
        .map_err(|e| format!("cargo build --all-targets hermetic failed: {e}"))?;
    if !status.success() {
        return Err("cargo build --all-targets hermetic falhou".into());
    }

    println!("\nOK: build hermético verificado. Próximos passos para SLSA L4 full");
    println!("    em docs/SLSA_L4_PROGRESS.md.");
    Ok(())
}

fn fuzz_replay(root: &Path) -> Result<(), String> {
    // Wrapper sobre `cargo test -p mcpix-core --test corpus_replay`. Existe
    // como comando dedicado para iteração local + para que o CI tenha um
    // ponto de entrada consistente — mudar de teste para binário no futuro
    // não quebra invocações externas.
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = std::process::Command::new(&cargo)
        .args(["test", "-p", "mcpix-core", "--test", "corpus_replay", "--", "--nocapture"])
        .current_dir(root)
        .status()
        .map_err(|e| format!("cargo test: {e}"))?;
    if !status.success() {
        return Err("fuzz corpus replay failed — see output above".into());
    }
    Ok(())
}

fn build_wasm(root: &Path) -> Result<(), String> {
    // 1. cargo build --release --target wasm32-unknown-unknown -p mcpix-wasm
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = std::process::Command::new(&cargo)
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "mcpix-wasm",
        ])
        .current_dir(root)
        .status()
        .map_err(|e| format!("cargo build wasm32: {e}"))?;
    if !status.success() {
        return Err("cargo build wasm32 failed".into());
    }

    // 2. wasm-bindgen --target web --out-dir examples/web-demo/pkg
    //    target/wasm32-unknown-unknown/release/mcpix_wasm.wasm
    let wasm_input = root
        .join("target/wasm32-unknown-unknown/release/mcpix_wasm.wasm");
    if !wasm_input.exists() {
        return Err(format!("wasm artifact not found at {}", wasm_input.display()));
    }
    let out_dir = root.join("examples/web-demo/pkg");
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("mkdir pkg/: {e}"))?;

    let status = std::process::Command::new("wasm-bindgen")
        .arg("--target")
        .arg("web")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg(&wasm_input)
        .current_dir(root)
        .status()
        .map_err(|e| format!("spawn wasm-bindgen (instale com `cargo install wasm-bindgen-cli --version 0.2.121`): {e}"))?;
    if !status.success() {
        return Err("wasm-bindgen failed".into());
    }

    println!("OK: examples/web-demo/pkg/ pronto. Sirva com `python3 -m http.server` em examples/web-demo/.");
    Ok(())
}

fn main() -> ExitCode {
    let cmd = env::args().nth(1);
    let root = workspace_root();
    let result = match cmd.as_deref() {
        Some("gen-bindings") => gen_bindings(&root),
        Some("check-bindings") => check_bindings(&root),
        Some("build-linux") => build_linux(&root),
        Some("build-windows") => build_windows(&root),
        Some("build-android") => build_android(&root),
        Some("build-ios") => build_ios(&root),
        Some("package-aar") => package_aar(&root),
        Some("package-xcframework") => package_xcframework(&root),
        Some("package-nuget") => package_nuget(&root),
        Some("build-all") => build_all(&root),
        Some("hash-artifacts") => hash_artifacts(&root),
        Some("gen-release-key") => gen_release_key(&root),
        Some("sign-artifacts") => sign_artifacts(&root),
        Some("build-wasm") => build_wasm(&root),
        Some("fuzz-replay") => fuzz_replay(&root),
        Some("verify-hermetic") => verify_hermetic(&root),
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
