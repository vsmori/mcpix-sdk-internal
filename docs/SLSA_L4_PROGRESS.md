# SLSA L4 — progresso e roadmap

[SLSA](https://slsa.dev/) L3 já foi atingido em S13 (provenance via
Sigstore Fulcio + Rekor). L4 adiciona dois requisitos:

1. **Build hermético**: nenhum acesso à rede durante a compilação,
   após o ponto de vendor das dependências.
2. **Build reproduzível**: dois runners independentes compilando o
   mesmo commit produzem **bytes idênticos** nos artefatos.

Este documento rastreia o que está pronto, o que falta e onde
investigar.

## Estado atual

| Requisito L4 | Status | Onde |
|---|---|---|
| Toolchain pinada | ✅ pronto | `rust-toolchain.toml` (channel = "stable", components fixos) |
| `Cargo.lock` versionada | ✅ pronto | `/Cargo.lock` no root do workspace |
| Build offline reproduzível | 🟡 ferramenta pronta, falta CI rotineiro | `cargo xtask verify-hermetic` |
| Catalogação de `build.rs` na dep tree | 🟡 inventário feito (abaixo); não-determinismos não auditados crate a crate | esta seção |
| Cross-runner hash comparison | 🟡 workflow pronto (manual) | `.github/workflows/reproducibility.yml` |
| Reproducibilidade de timestamps em artefatos | 🟡 `SOURCE_DATE_EPOCH` plumbed para `cargo build` | reproducibility.yml expõe via env |

## Como rodar o build hermético localmente

```bash
cargo xtask verify-hermetic
```

O comando:

1. `cargo vendor --locked vendor` — baixa todas as 184 crates
   transitive para `./vendor/`.
2. Escreve `.cargo/config.hermetic.toml` apontando o source
   `crates-io` para `./vendor/`.
3. `cargo build --workspace --frozen --locked --offline --config
   "include=[…]"` — falha se qualquer dep tentar acessar a rede ou
   se a versão diverge do `Cargo.lock`.
4. Repete com `--all-targets` para cobrir testes/exemplos/benches.

Falha = não hermético. Sucesso = "este commit consegue construir
isolado da rede".

## Build.rs no grafo de dependências

184 crates transitivas. **67** delas têm `build.rs`. Esta é a
superfície de não-determinismo: qualquer destes scripts pode ler
relógio, env vars, hostname, ou tentar rede.

### Categoria A — risco baixo, deterministas conhecidos

Crates de macro / declaração / suporte a derive cujo `build.rs` só
detecta features do compilador via `rustc --version`:

`anyhow`, `proc-macro2`, `quote`, `serde`, `serde_core`,
`serde_json`, `thiserror-{1,2}`, `paste`, `prettyplease`,
`num-traits`, `camino`, `parking_lot_core`, `fs-err-{2,3}`,
`generic-array`, `httparse`, `oid-registry`, `mime_guess`,
`zerocopy`, `wit-bindgen-{0.51,0.57}`, `wit-bindgen-rust*`,
`zmij`.

Todos consultam só `rustc-cfg` declarações estáticas. **Determinísticos.**

### Categoria B — feature detection de plataforma

`build.rs` consulta `target_arch` / `target_os` para emitir
`cfg(...)`:

`ahash`, `getrandom-{0.3,0.4}`, `heapless`, `jni-sys`, `libc`,
`rustix`, `windows_*` (todas as 20 variants — uma por plataforma),
`curve25519-dalek`, `wasm-bindgen`, `wasm-bindgen-shared`.

**Determinísticos** para um mesmo target. O hash do artefato Linux
pode diferir do Windows — *isso é esperado*; o que L4 exige é que
duas builds Linux x86_64 produzam o mesmo `.so`.

### Categoria C — risco alto, exige investigação

| Crate | Por que requer atenção |
|---|---|
| `aws-lc-rs-1.17.0` | Bundle de AWS-LC C library, linkagem nativa. **Não está em uso** pelo workspace default (usamos `ring` via rustls), mas aparece via `rustls-platform-verifier` transitivo. Risco baixo na prática se não habilitamos a feature `aws-lc` do rustls. |
| `cbindgen-0.27.0` | Geração de bindings C. Executa só em `gen-bindings`, não no build do `.so`. Output é commitado (`bindings/c/include/mcpix.h`). |
| `libsqlite3-sys-0.28.0` | Compila ou linka SQLite via `cc`. Determinístico se `SQLITE_*` env vars forem fixas. Ativo só com feature `sqlite` do `mcpix-receiver-sdk`. |
| `quinn-{0.11.9}`, `quinn-udp-0.5.14` | Detecta features de UDP do kernel via `uname`. Transitivo via reqwest/rustls-platform-verifier; tipicamente não usado nos paths críticos da SDK. |
| `ring-0.17.14` | Assembly por arquitetura. **Determinístico** desde que o mesmo target seja usado. Auditado pelo upstream para reprodutibilidade. |
| `rustls-0.23.40` | `build.rs` checa nightly features (não usamos), sem leitura de I/O. |
| `icu_normalizer_data-2.2.0`, `icu_properties_data-2.2.0` | Embedam tabelas Unicode estáticas via `OUT_DIR`. Determinístico desde que mesma versão. |

### Categoria D — depende de tooling externo

`libc`, `rustix` — feature detection do kernel via `cc` para
test programs. Determinístico se a versão do `cc` invocado também
for pinada. **Não pinada hoje** — gap conhecido.

## Roadmap para L4 full

### Curto prazo (próxima sessão de 2-3h)

1. **CI step rotineiro de `verify-hermetic`** no `release.yml` —
   antes de cada release tag, prova que o build do release é
   hermético. Falha se algum dep mudou e o vendor ficou stale.
2. **Pin do `cc` toolchain** no `release.yml` via container image
   ou step explícito (`apt-get install -y g++=...`).
3. **Executar `reproducibility.yml` periodicamente** (e.g. agendado
   semanal). O workflow existe em modo `workflow_dispatch` — para
   se tornar gate, basta adicionar `schedule:` ou exigir success no
   PR (custo: ~10min de CI por execução).

### Médio prazo (sessão dedicada)

3. ~~**Cross-runner hash comparison**~~: **entregue** como
   `reproducibility.yml`. Dois runners (`same` ou
   `cross-ubuntu-version`) compilam `libmcpix_ffi.so` e comparam
   sha256. Em falha, roda `diffoscope` para identificar a fonte do
   divergir. Escopo atual: só Linux x86_64; AAR/NuGet exigem
   tarball-determinismo separado.
4. **`SOURCE_DATE_EPOCH`** universal: definir env var no CI para
   que tar/zip embebidos (AAR/NuGet) usem timestamp determinístico
   em vez do clock do runner.
5. **Auditar Categoria C** crate-a-crate: ler o `build.rs` real de
   `libsqlite3-sys` etc. e classificar como "determinístico" ou
   "requer mitigação".

### Longo prazo

6. **Mirror local de crates.io**: para fugir do risco de
   crates.io renomear/yanked uma versão entre `cargo vendor` e
   `cargo build`. Hoje confiamos no checksum do `Cargo.lock`, que
   é suficiente para a propriedade de L4 mas não para
   sobrevivência total ao registry offline.

## Por que isto não está em S13

S13 (SLSA L3) era *attestation* — provar que um runner concreto
do GitHub Actions emitiu aquele binário. L4 é *bytewise
reprodutibilidade* — provar que **qualquer** runner emite o mesmo
binário. As duas propriedades se compõem: L3 + L4 = consumidor
pode rebuildar localmente e bater o hash do `.intoto.jsonl` que
ele já confiava via Sigstore.

## Anchors

- `xtask verify-hermetic` — entrypoint operacional.
- `rust-toolchain.toml` — compilador pinado.
- `Cargo.lock` — versões pinadas (já versionado pré-S13).
- `docs/SLSA.md` — referência da L3.
