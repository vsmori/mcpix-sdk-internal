# CONTRIBUTING

Setup para desenvolver na SDK. Para apenas **usar**, veja
[`QUICKSTART.md`](QUICKSTART.md).

## Setup inicial

```bash
# Clone + dependências
git clone https://github.com/vsmori/mcpix-sdk-internal
cd mcpix-sdk-internal

# Ativa o pre-commit hook (versionado em .githooks/)
git config core.hooksPath .githooks

# Smoke
cargo test --workspace
```

O pre-commit hook roda `cargo fmt --check` + `cargo clippy --workspace`
antes de cada commit — replica o gate de `ci.yml`. Para skipar num
commit pontual: `git commit --no-verify` (use raramente).

## Comandos diários

```bash
cargo test --workspace                  # 112 testes default
cargo test -p <crate> --features <f>    # testes por feature
cargo fmt --all                          # auto-format
cargo clippy --workspace --all-targets   # lints
cargo xtask fuzz-replay                  # replay do corpus versionado
```

Suite por feature (todas em CI):

| Crate | Feature | Testes |
|---|---|---|
| `mcpix-receiver-sdk` | `sqlite` | persistência local |
| `mcpix-receiver-sdk` | `sealed-store` | Seed selada com ChaCha20-Poly1305 |
| `mcpix-bank-receiver` | `http-server,http-client` | HTTP E2E |
| `mcpix-bank-receiver` | `mtls` | TLS mútuo + CRL |
| `mcpix-bank-receiver` | `ocsp,http-server` | Live OCSP query |
| `mcpix-embed` | `qr,storage,restore` | embarcado completo |
| `mcpix-embed` | `restore` | cross-validate com `mcpix-backup` |

## Antes de abrir PR

1. `cargo fmt --all` — o CI rejeita drift.
2. `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings.
3. `cargo test --workspace` + a feature que você tocou.
4. Se modificou wire format / API pública: atualize ADRs em
   `docs/adr/` ou abra uma nova.
5. Se modificou erro / código FFI: rode `cargo xtask gen-bindings` e
   commite os bindings regenerados junto.

## Estrutura do repo

```
crates/
├── mcpix-core              núcleo cripto + tipos, sem I/O
├── mcpix-receiver-sdk      fachada Rust (Counter, SeedStore, etc)
├── mcpix-bank-receiver     servidor HTTP/mTLS do banco do recebedor
├── mcpix-bank-payer-mock   simulação do banco do pagador (testes)
├── mcpix-ffi               C-ABI manual (.NET P/Invoke)
├── mcpix-uniffi            scaffolding UniFFI (Swift, Kotlin)
├── mcpix-embed             subset no_std para microcontroladores
├── mcpix-backup            backup criptografado de sementes
└── mcpix-wasm              bindings WebAssembly para a demo browser
embedded/                   bare-metal demo (Cortex-M4F)
examples/                   samples por plataforma
fuzz/                       libfuzzer + corpus versionado
xtask/                      automação (build cross, hash, fmt)
bindings/                   binding sources (C, dotnet, swift, kotlin)
.github/workflows/          CI (ci, release, build, fuzz, samples, etc)
docs/                       ADRs, PROTOCOL, THREAT_MODEL, etc
```

## Adicionando uma ADR

Decisões arquiteturais novas viram ADRs numeradas em `docs/adr/`. Use
[`0008-mtls-san-uri-identity.md`](docs/adr/0008-mtls-san-uri-identity.md)
como template. Seções: Status, Contexto, Decisão, Alternativas
consideradas, Consequências (positivas/negativas), Validação,
Referências.

## Versionamento

- **Cargo crates**: semver, ainda em `0.1.x` (pre-1.0 — quebras
  esperadas, mas documentadas no CHANGELOG).
- **Protocolo wire (`PIXOFFv1`)**: política completa em
  [`docs/VERSIONING.md`](docs/VERSIONING.md). Bumps de versão wire
  exigem soak window de ≥18 meses.

## Convenções de commit

Mensagens em inglês, no presente do indicativo, primeira linha ≤72
chars. Padrão tipo Conventional Commits sem rigor de tags — o que
importa é explicar o **porquê** no body (parágrafo após linha em
branco), não o **o quê** (o diff mostra).

Exemplos no `git log`. Não force squash em PRs grandes — manter os
commits intermediários ajuda na revisão e no `git bisect`.

## Onde está o que

| Quero... | Vá para |
|---|---|
| Mexer no protocolo / cripto | `crates/mcpix-core` |
| Adicionar feature à fachada Rust | `crates/mcpix-receiver-sdk` |
| Adicionar endpoint HTTP | `crates/mcpix-bank-receiver/src/http_server.rs` |
| Expor algo novo aos bindings nativos | `crates/mcpix-uniffi/src/lib.rs` |
| Adicionar comando de build | `xtask/src/main.rs` |
| Documentar nova decisão | nova ADR em `docs/adr/` |

## Ciclo de release

Tags `v*` disparam `release.yml` que produz cross-platform artifacts
+ SLSA L3 provenance + GitHub Release. Antes de cortar uma tag:

1. CHANGELOG atualizado.
2. `cargo xtask verify-hermetic` localmente (validação manual de L4).
3. Crie tag anotada: `git tag -a v0.2.0 -m "release v0.2.0"` + push.
4. Aguarde o `release.yml` (15-20 min) — Release aparece em
   `https://github.com/vsmori/mcpix-sdk-internal/releases`.

## Onde pedir ajuda

- Issues: para bugs reproduzíveis ou requests de feature.
- Discussions (futuro): para perguntas de uso.
- Docs em `docs/`: respondem 90% das dúvidas sobre design.
