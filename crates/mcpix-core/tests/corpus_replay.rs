//! Replay determinístico do corpus de fuzzing — roda **a cada push**
//! sem precisar de nightly, libfuzzer ou cargo-fuzz.
//!
//! Cada arquivo em `fuzz/corpus/<target>` e `fuzz/regression/<target>`
//! é passado pelo parser/verificador correspondente. Invariantes:
//!
//! - Nenhum input pode produzir panic. A política de não-pânico do
//!   Bloco 1.3 da especificação exige isso para inputs adversariais.
//! - Para `fuzz_verify_combined`, nenhum input pode resultar em
//!   `SignatureCheck::Verified` — bytes aleatórios verificando seria
//!   forjamento gratuito.
//!
//! Quando o CI do `fuzz.yml` descobre um novo crash:
//! 1. Triage manual.
//! 2. Move o arquivo de `fuzz/artifacts/` para `fuzz/regression/`.
//! 3. Fix o código.
//! 4. Este teste passa de novo, e quebraria se o fix regredisse.

use std::fs;
use std::path::{Path, PathBuf};

use arbitrary::{Arbitrary, Unstructured};

use mcpix_core::signature::parse_sums_line;
use mcpix_core::signature::{verify_combined, RELEASE_PUBKEY};
use mcpix_core::transport_field::parse as parse_transport;

// ─────────────────────────────────────────────────────────────────────
// Resolução de paths: o teste roda com CWD = crates/mcpix-core/. Vamos
// duas pastas acima para chegar em fuzz/.
// ─────────────────────────────────────────────────────────────────────

fn fuzz_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("fuzz"))
        .expect("fuzz/ directory expected at workspace root")
}

/// Itera arquivos em `<fuzz_root>/corpus/<target>` E
/// `<fuzz_root>/regression/<target>`. Ambos categorizados juntos para
/// que a invariante valha em qualquer entrada conhecida, seja seed
/// descoberta automaticamente ou caso curado.
fn collect_inputs(target: &str) -> Vec<(PathBuf, Vec<u8>)> {
    let fuzz = fuzz_root();
    let mut out = Vec::new();
    for category in &["corpus", "regression"] {
        let dir = fuzz.join(category).join(target);
        if !dir.exists() {
            continue;
        }
        for entry in fs::read_dir(&dir).expect("read_dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            // .gitkeep e README são metadados, não inputs.
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name.ends_with(".md") {
                    continue;
                }
            }
            let bytes = fs::read(&path).expect("read file");
            out.push((path, bytes));
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────
// fuzz_transport_parse — espelha fuzz_targets/fuzz_transport_parse.rs
// ─────────────────────────────────────────────────────────────────────

#[test]
fn replay_fuzz_transport_parse_no_panic() {
    let inputs = collect_inputs("fuzz_transport_parse");
    assert!(
        !inputs.is_empty(),
        "fuzz_transport_parse corpus is empty — replay would be vacuous"
    );
    for (path, bytes) in inputs {
        if let Ok(s) = std::str::from_utf8(&bytes) {
            // Não importa o resultado — só importa que não panica.
            let _ = parse_transport(s);
        }
        // Bytes inválidos como UTF-8 são caso esperado de input não-input.
        let _ = path;
    }
}

// ─────────────────────────────────────────────────────────────────────
// fuzz_sums_line — espelha fuzz_targets/fuzz_sums_line.rs
// ─────────────────────────────────────────────────────────────────────

#[test]
fn replay_fuzz_sums_line_no_panic() {
    let inputs = collect_inputs("fuzz_sums_line");
    assert!(
        !inputs.is_empty(),
        "fuzz_sums_line corpus is empty — replay would be vacuous"
    );
    for (_, bytes) in inputs {
        if let Ok(s) = std::str::from_utf8(&bytes) {
            let _ = parse_sums_line(s);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// fuzz_verify_combined — espelha fuzz_targets/fuzz_verify_combined.rs.
// Aqui o target usa #[derive(arbitrary::Arbitrary)] para reconstituir
// uma struct a partir de bytes opacos. Reproduzimos a mesma decodificação
// via `arbitrary::Unstructured` — sem libfuzzer, sem nightly.
// ─────────────────────────────────────────────────────────────────────

#[derive(Arbitrary, Debug)]
struct VerifyInput<'a> {
    sums: &'a [u8],
    sig: &'a [u8],
    filename: &'a str,
    hash_hex: &'a str,
}

#[test]
fn replay_fuzz_verify_combined_no_panic_and_no_forgery() {
    use mcpix_core::signature::SignatureCheck;
    let inputs = collect_inputs("fuzz_verify_combined");
    assert!(
        !inputs.is_empty(),
        "fuzz_verify_combined corpus is empty — replay would be vacuous"
    );
    for (path, bytes) in inputs {
        let mut u = Unstructured::new(&bytes);
        // Nem todo input bate o schema de `VerifyInput` (o `arbitrary`
        // pode falhar se faltar bytes). Esses inputs simplesmente são
        // skipped pelo libfuzzer também.
        if let Ok(input) = VerifyInput::arbitrary(&mut u) {
            let result = verify_combined(
                input.sums,
                input.sig,
                RELEASE_PUBKEY,
                input.filename,
                input.hash_hex,
            );
            if let Ok(SignatureCheck::Verified) = result {
                panic!(
                    "FORGERY: corpus input {} produced Verified — incident inicia aqui",
                    path.display()
                );
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Smoke: o corpus deve estar acessível. Se `fuzz/corpus/` sumir, este
// teste falha com mensagem clara em vez dos três acima panicarem em
// `assert!(!inputs.is_empty())`.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn corpus_directory_exists() {
    let root = fuzz_root();
    assert!(
        root.join("corpus").exists(),
        "expected versioned corpus at {}",
        root.join("corpus").display()
    );
    for target in &[
        "fuzz_transport_parse",
        "fuzz_sums_line",
        "fuzz_verify_combined",
    ] {
        let dir = root.join("corpus").join(target);
        assert!(
            dir.exists(),
            "corpus directory missing for target {target}: {}",
            dir.display()
        );
    }
}
