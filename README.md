# mcpix-sdk-internal

[![ci](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/ci.yml/badge.svg)](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/ci.yml)
[![samples (mobile)](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/samples-mobile.yml/badge.svg)](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/samples-mobile.yml)
[![fuzz](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/fuzz.yml/badge.svg)](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/fuzz.yml)
[![reproducibility](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/reproducibility.yml/badge.svg)](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/reproducibility.yml)
[![release](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/release.yml/badge.svg)](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/release.yml)
[![build](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/build.yml/badge.svg)](https://github.com/vsmori/mcpix-sdk-internal/actions/workflows/build.yml)

SDK Rust de validação local de transações de pagamento. Pré-depósito PCT — uso interno.

> **Novo aqui?** Comece pelo [`QUICKSTART.md`](QUICKSTART.md) — 5 minutos até
> ver código rodando. Para contribuir, [`CONTRIBUTING.md`](CONTRIBUTING.md).

📚 **Documentação técnica completa**: ver [`docs/`](./docs/) — arquitetura,
protocolo, modelo de ameaças, especificação criptográfica, mapeamento das
reivindicações PCT, glossário e 10 ADRs.

## Estrutura

```
crates/
  mcpix-core              núcleo criptográfico, sem I/O
  mcpix-receiver-sdk      SDK do recebedor (compõe núcleo + stores locais)
  mcpix-bank-receiver     módulo backend do banco recebedor (custódia de sementes)
  mcpix-bank-payer-mock   simulação do banco do pagador (substituição institucional)
  mcpix-ffi               C-ABI manual para .NET P/Invoke; gera bindings/c/include/mcpix.h
  mcpix-uniffi            scaffolding UniFFI para Swift e Kotlin
  uniffi-bindgen          wrapper de versão fixa do gerador UniFFI
examples/
  e2e_demo.rs             demo CLI dos 3 módulos lado a lado
bindings/
  c/include/mcpix.h       header gerado por cbindgen (commitado)
  swift/                  Swift Package + bindings gerados por UniFFI
  kotlin/                 Gradle project + bindings gerados por UniFFI
xtask/                    cargo xtask gen-bindings | check-bindings
```

## Executar

```bash
# testes Rust (default + sqlite + HTTP + integração contra dist/ + propriedades)
cargo test --workspace
cargo test --workspace --features mcpix-receiver-sdk/sqlite
cargo test -p mcpix-bank-receiver --features http-server,http-client
cargo test -p mcpix-bank-receiver --features mtls
cargo test -p mcpix-embed --features qr
cargo test -p mcpix-embed --features qr,storage

# bare-metal builds (validar no_std)
cargo build -p mcpix-embed --no-default-features --features qr \
  --target thumbv7em-none-eabihf --release
cargo build -p mcpix-embed --no-default-features --features qr \
  --target riscv32imc-unknown-none-elf --release

# binário Cortex-M4F completo (demo no embedded/)
(cd embedded && cargo build --release)
arm-none-eabi-size embedded/target/thumbv7em-none-eabihf/release/mcpix-embed-demo
cargo test --workspace -- --include-ignored
PROPTEST_CASES=10000 cargo test -p mcpix-core --test properties --release

# fuzzing (precisa nightly + cargo-fuzz)
cargo +nightly fuzz run fuzz_transport_parse -- -max_total_time=60
cargo +nightly fuzz run fuzz_sums_line -- -max_total_time=60
cargo +nightly fuzz run fuzz_verify_combined -- -max_total_time=60

# demo CLI end-to-end
cargo run -p mcpix-examples --bin e2e_demo

# bindings Swift/Kotlin/.h (gerados, commitados)
cargo xtask gen-bindings
cargo xtask check-bindings        # CI: falha se houver drift

# smoke test do binding Kotlin contra o cdylib
cargo build -p mcpix-uniffi
(cd bindings/kotlin && gradle test)

# cross-compile e empacotamento
cargo xtask build-linux           # x86_64-unknown-linux-gnu → dist/linux-x86_64/
cargo xtask build-windows         # x86_64-pc-windows-gnu (requer mingw-w64)
cargo xtask build-android         # 4 ABIs via cargo-ndk (requer ANDROID_NDK_HOME)
cargo xtask build-ios             # iOS device + sim (macOS host)
cargo xtask package-aar           # .aar via gradle :aar:assembleRelease
cargo xtask package-xcframework   # MCPixSDKFFI.xcframework (macOS host)
cargo xtask package-nuget         # .nupkg via dotnet pack
cargo xtask build-all             # tudo aplicável ao host
cargo xtask hash-artifacts        # dist/SHA256SUMS sobre todo dist/

# assinatura digital (S4)
cargo xtask gen-release-key       # gera novo par Ed25519 (uma vez por rotação)
MCPIX_SIGN_PRIVKEY_HEX=<hex> cargo xtask sign-artifacts  # gera SHA256SUMS.sig
```

## Builds sob demanda no GitHub Actions

Existem duas pipelines de artefato no `.github/workflows/`:

| Workflow | Trigger | Saída |
|---|---|---|
| `release.yml` | push de tag `v*` | GitHub Release oficial assinado |
| `build.yml`   | `workflow_dispatch` (manual) | Artefatos GHA com label `<versão>-<sha>` |

Use `build.yml` quando precisar do `.so / .dll / .aar / .nupkg / .xcframework`
de um commit qualquer sem criar release. Disparo via `gh`:

```bash
# Tudo + SLSA provenance (default)
gh workflow run build.yml -f target=all -f provenance=true

# Só o .aar de Android (rápido, só roda o que importa)
gh workflow run build.yml -f target=aar -f provenance=true

# Linux .so sem provenance (debug iteration)
gh workflow run build.yml -f target=linux -f provenance=false -f retention_days=7
```

Alvos disponíveis: `all`, `linux`, `windows`, `android`, `ios`, `aar`,
`nuget`. Cada um arrasta exatamente as dependências mínimas (e.g.
`aar` puxa `android`; `nuget` puxa `linux`+`windows`). Provenance é
keyless via Sigstore Fulcio + Rekor (mesma cadeia do `release.yml`,
documentada em `docs/SLSA.md`).


## Exemplos por plataforma

Samples completos consumindo a SDK em cada stack — ponto de partida
para integradores. Cada um exercita o fluxo do recebedor (register →
generate → validate). Tabela completa + comandos build/run em
[`examples/README.md`](examples/README.md):

| Plataforma | Pasta |
|---|---|
| Rust host (CLI completa, recebedor+pagador) | [`examples/e2e_demo.rs`](examples/e2e_demo.rs) |
| Browser (WASM, side-by-side bancos) | [`examples/web-demo/`](examples/web-demo/) |
| .NET 8 (P/Invoke) | [`examples/dotnet-sample/`](examples/dotnet-sample/) |
| Kotlin JVM (JNA, CLI) | [`examples/kotlin-jvm-sample/`](examples/kotlin-jvm-sample/) |
| Android (Activity + AAR) | [`examples/android-sample/`](examples/android-sample/) |
| iOS (SwiftUI + XCFramework) | [`examples/ios-sample/`](examples/ios-sample/) |
| Apple Wallet + App Clip (geração offline + QR + NFC) | [`examples/apple-wallet-appclip/`](examples/apple-wallet-appclip/) |
| Google Wallet + Play Instant (geração offline + QR + NFC) | [`examples/google-wallet-instant-app/`](examples/google-wallet-instant-app/) |
| Bare-metal Cortex-M4F (`no_std`) | [`embedded/`](embedded/) (apontado por [`examples/embedded-demo/`](examples/embedded-demo/)) |

## Self-check de integridade (S3 + S4)

Duas camadas de defesa:

**S3 — SHA-256 self-check.** `mcpix_core::integrity::verify_bytes` compara o
hash do binário carregado com `MCPIX_EXPECTED_SHA256` carimbado em build time.
Detecta substituição/patch do binário. *Vulnerável* a atacante que recompile e
re-carimbe o hash.

**S4 — Manifesto assinado (Ed25519).** O CI assina `dist/SHA256SUMS` produzindo
`dist/SHA256SUMS.sig` com chave privada em secret. A chave pública canônica está
em `crates/mcpix-core/trusted_keys/release.pub` (32 bytes raw, commitada,
embarcada via `include_bytes!`). `verify_self()` agora:

1. Localiza `SHA256SUMS` + `SHA256SUMS.sig` ao lado do binário (ou no parent).
2. Valida assinatura com `RELEASE_PUBKEY`.
3. Procura o nome do próprio binário em `SHA256SUMS`.
4. Compara hash. Retorna `Verified` / `Tampered`.

Política:
- **Release build** (com `MCPIX_EXPECTED_SHA256` carimbado) + manifest assinado presente
  → exige verificação completa; ausência ou assinatura inválida ⇒ `Tampered`.
- **Dev build** ⇒ `Skipped` (não derruba dev por falta de manifest).
- Caller (fachada Swift/Kotlin/.NET) deve abortar inicialização em `Tampered`.

### Rotação de chave

```bash
rm crates/mcpix-core/trusted_keys/release.pub
cargo xtask gen-release-key        # imprime priv hex uma vez
# copie a priv para o secret MCPIX_SIGN_PRIVKEY_HEX e backup offline
git add crates/mcpix-core/trusted_keys/release.pub
git commit -m "rotate release signing key"
```

### Cobertura

- 10 unit tests em `mcpix_core::signature` (good sig, bad sig, wrong key,
  malformed sums, hash mismatch, file absent, pubkey well-formed)
- 5 integration tests em `tests/integrity_against_dist.rs` contra o `.so` real
  e o manifest assinado (round-trip, tampering, swap, sums tampered)

## Cobertura de testes

**Sessão 1** (núcleo Rust)
- determinismo `(S, T) → (C₁, C₂)`
- validação positiva e negativa de `C₂`
- defesa de replay (segunda apresentação rejeitada)
- comparação em tempo constante via `subtle::ConstantTimeEq`
- parse/encode do campo de transporte (`PIXOFFv1` + SeedId-16 + C₁-11 = 35 chars)
- round-trip do `SqliteSeedStore` (feature `sqlite`)
- fluxo completo via FFI C-ABI (`mcpix_receiver_*`)

**Sessão 2** (bindings)
- geração reprodutível de header C, Swift e Kotlin (`check-bindings`)
- smoke test JVM: carregamento do `libmcpix_uniffi.so` via JNA, fluxo
  `register → generate_charge → validate_receipt` e tipagem de erro
  (`McpixUniffiException.InvalidSeedId`)

**Sessão 3** (distribuição)
- self-check SHA-256 do binário (`mcpix_core::integrity::verify_bytes`),
  com `MCPIX_EXPECTED_SHA256` injetado em release via build script
- `cargo xtask` cobre Linux, Windows, Android (cargo-ndk), iOS (macOS host),
  empacotamento AAR/XCFramework/NuGet e geração de `SHA256SUMS`
- GitHub Actions: `ci.yml` (testes + clippy + bindings drift + Kotlin JVM)
  e `release.yml` (matriz Linux/macOS, publicação opcional em Maven/NuGet)

**Sessão 4** (cadeia de confiança)
- Ed25519 release key embarcada via `include_bytes!`
- `xtask gen-release-key` + `xtask sign-artifacts` + verificação combinada
  no runtime (assinatura + hash do manifest)
- CI assina `SHA256SUMS` quando `MCPIX_SIGN_PRIVKEY_HEX` está presente

**Sessão 5** (rigor cripto: property-based + fuzzing)
- 14 propriedades em `tests/properties.rs` rodando 2k+ casos cada cobrem
  determinismo, encadeamento C₁→C₂, distinção por seed/counter, codificação
  alfanumérica, round-trip do campo de transporte, parsing robusto, equivalência
  da comparação em tempo constante e ausência de forgery aleatório de assinatura
- 3 fuzz targets em `fuzz/` via cargo-fuzz: `fuzz_transport_parse`,
  `fuzz_sums_line`, `fuzz_verify_combined`. 25M+ execuções locais sem
  encontrar panic ou forgery
- `.github/workflows/fuzz.yml`: 1 min por target em PR, 30 min em schedule
  semanal, com upload de crash artifacts em falha

**Sessão 6** (T = timestamp quantizado, fechando gap com a reivindicação PCT)
- `TimestampQuantizedCounter` (janela default 30s, RFC 6238 style) ao lado
  do `InMemoryCounter` sequencial — ambos satisfazem o trait `Counter`
- Garantias enforçadas: monotonia, anti-colisão no mesmo quantum
  (`McpixError::CounterCollision`), anti-rollback de relógio
  (`McpixError::CounterRollback`)
- `SystemClock` e `TestClock` concretos para a trait `Clock` do núcleo
- `PayerBankMock::process_payment_windowed` emite 2N+1 candidatos C₂ para
  tolerar drift de até N janelas entre recebedor e banco
- Demo `e2e_demo` estendido com PARTE B mostrando colisão dentro da janela,
  avanço de relógio e validação com tolerância
- 11 testes novos cobrindo os caminhos de quantização e tolerância

**Sessão 7** (HTTP transport + REST endpoints — bank-receiver real)
- `mcpix-bank-receiver` ganha features `http-server` (axum) e
  `http-client` (reqwest blocking) — opcionais, default permanece zero-rede
- Endpoints REST: `POST /v1/seeds/{seed_id}`, `GET /v1/seeds/{seed_id}`,
  `GET /v1/healthz`. JSON payload com material em base64. Header
  `X-Institution-Id` como placeholder para mTLS futuro
- `HttpBankReceiver` implementa o trait `BankReceiver` contra o servidor
  — é injetável diretamente em `PayerBankMock`, mantendo o contrato sync
- 3 integration tests em `tests/http_e2e.rs` que spawnam servidor real
  em porta aleatória loopback e exercitam o protocolo completo via HTTP
  (recebedor offline → REST → banco do pagador → recompõe C₂ → valida)

**Sessão 8** (mTLS real — fim do header placeholder)
- Nova feature `mtls` empilha sobre `http-server` + `http-client`,
  substituindo `axum::serve` por `axum-server::from_tcp_rustls` com
  `WebPkiClientVerifier` em modo obrigatório
- `mtls::extract_institution_id` extrai a identidade da instituição do
  SAN URI (`urn:mcpix:institution:<id>`) do cert do cliente, com
  fallback para CN — substitui o header `X-Institution-Id` da S7
- `MtlsClientMaterial` + `build_mtls_client` montam `reqwest::blocking`
  com client cert PEM + CA do servidor; entrega-se diretamente como
  argumento de `HttpBankReceiver::with_client`
- 4 integration tests em `tests/mtls_e2e.rs` que geram PKI in-process
  via `rcgen` (CA + cert servidor com SAN DNS+IP + cert cliente com SAN
  URI) e cobrem: round-trip OK; rejeição de cliente sem cert; rejeição
  de cliente assinado por CA não-confiada; extração de identidade do
  cert DER

**Sessão 16** (versionamento do protocolo)
- Novo módulo `mcpix_core::version` com enum `ProtocolVersion` (`V1 = 1`)
  + `detect()` + `is_any_version()`; constantes `PROTOCOL_PREFIX_FAMILY`
  e `PROTOCOL_PREFIX_LEN` centralizadas
- `transport_field::parse` dispatcha por versão antes do parsing
  posicional — versão desconhecida nunca é interpretada com regras de
  outra versão
- Novo erro `McpixError::UnsupportedProtocolVersion(String)` distingue
  "SDK desatualizado" (prefixo `PIXOFFv*` futuro) de "não é nosso
  protocolo" (prefixo fora da família). FFI status code = 15
- `ParsedField` ganha `version: ProtocolVersion`; `encode_with_version()`
  permite emissão explícita (default fica em `current() = V1`)
- Estrutura aditiva: introduzir V2 é um arquivo paralelo `parse_v2` e
  uma variante no enum — código V1 nunca precisa mudar
- 14 testes novos (8 em version.rs, 4 em transport_field.rs, mais
  invariantes ABI ancorados) — total 101 testes default
- Bindings C/C# atualizados; uniffi mapeia `UnsupportedProtocolVersion`
  → `TransportField` (compat de assinatura)
- Política completa em `docs/VERSIONING.md`: triggers de bump, janela
  de coexistência ≥18 meses, ABI invariants imutáveis

**Sessão 15** (demo browser via WebAssembly)
- Nova crate `mcpix-wasm` com bindings `wasm-bindgen` cobrindo ambos os
  lados do protocolo num único módulo wasm de ~80 KB
- `examples/web-demo/index.html` single-page sem framework com layout
  side-by-side (recebedor + pagador), código colorizado por
  público/secreto, log de operações, replay detection visual
- `getrandom = { features = ["js"] }` plumba `crypto.getRandomValues`
  para `rand_core::OsRng` — `Seed` gerada com entropia real do browser
- `cargo xtask build-wasm` reprodutível; CI valida que o bundle final
  fica abaixo de 200 KB (smoke contra blob inflation)
- 3 testes host-side mirram o fluxo da demo (substituição institucional
  bit-exata, replay rejeitado, tampering detectado)

**Sessão 14** (revogação de certificados mTLS — CRL + OCSP stapling)
- `ServerTlsConfig` (novo): builder com `with_client_crls(pem)` e
  `with_stapled_ocsp(der)`. CRL valida client certs apresentados;
  stapling anexa OCSP response ao server cert
- `MtlsClientMaterial::with_server_crls(pem)`: cliente passa a usar
  `rustls::ClientConfig` custom com `WebPkiServerVerifier` + CRLs,
  rejeitando server certs revogados
- CRL expirada (`nextUpdate` passado) ou com assinatura quebrada falha
  no build do verifier — não há janela silenciosa
- 3 testes E2E novos com CRL real gerada via rcgen: revogar client,
  CRL vazia (não-falso-positivo), revogar server. Suite mTLS: 7 testes
- Fecha `THREAT_MODEL.md` §6.5 (revogação); operacional em novo
  `docs/MTLS_REVOCATION.md`

**Sessão 13** (SLSA L3 supply-chain provenance)
- `release.yml` ganha job `provenance` que invoca o reusable workflow
  oficial `slsa-framework/slsa-github-generator` em runner separado
- Emite `mcpix-sdk.intoto.jsonl` com predicate `slsa-provenance` v1,
  assinatura keyless via Sigstore Fulcio (cert efêmero ligado ao OIDC
  do GitHub Actions) e inclusion proof em Rekor transparency log
- Consumer verifica com `slsa-verifier verify-artifact ...` antes de
  carregar — vincula bit-exato o `.so/.dll/.aar` ao commit fonte
- Novo `docs/SLSA.md` documenta verificação manual e em CI
- Novo `scripts/verify-release.sh` automatiza verificação em lote de
  todos os artefatos da release
- Fecha §5.3 do THREAT_MODEL (Comprometimento do CI) — atacante teria
  que comprometer o GitHub IdP para forjar OIDC, não basta runner

**Sessão 12** (backup/restore criptografado de sementes)
- Nova crate `mcpix-backup` (host): export/import de `Seed + SeedId +
  counter_mode + counter_t` cifrado com Argon2id + ChaCha20-Poly1305,
  serializado em Base58Check (single line, alfanumérico, anti-typo)
- Container 122 bytes (header AAD 47 + ciphertext 59 + tag 16) → ~167
  chars Base58Check. Cabe em uma linha impressa ou QR code
- Header autenticado como AAD da AEAD: tampering nos parâmetros KDF
  (ex. reduzir m_cost) quebra a tag
- `mcpix-embed` ganha feature `restore` com import paralelo no_std
  (paridade bit-exata do wire format via cross-validation)
- 14 testes novos: 11 unit em `mcpix-backup` + 3 cross-validate
  host↔embed
- Defesas: wrong passphrase ⇒ `DecryptFailed`; 1-bit tampering quebra
  AEAD; salts/nonces independentes evitam que outputs idênticos vazem
  o mesmo input

**Sessão 11** (persistência embarcada de C₂ e contador T)
- `mcpix-embed` ganha feature `storage` com `ReceiptStore` e
  `CounterStore` sobre `embedded-storage::NorFlash`
- Ping-pong de 2 slots por record type: anti-corrupção via CRC32, wear-leveling
  básico, sobrevivência a queda de energia durante write
- `RamFlash<N>` provida para testes/demo simulando NOR flash (write
  apenas clear bits, erase volta a 0xFF) — pega bugs onde caller
  esquece erase
- Demo bare-metal estendida: generate → save → simula reboot → load →
  validate → mark_consumed. Binário Cortex-M4F: 20,796 bytes (+4.2 KB
  vs S9, ainda confortável)
- 7 testes em `storage::tests` cobrindo: round-trip; empty; ping-pong
  no segundo save; consumed sobrevive reboot; corrupção em 1 slot não
  derruba o outro; counter store; coexistência de ambos em mesma flash
- THREAT_MODEL §7.2 atualizado: gap reconhecido → coberto

**Sessão 10** (documentação arquitetural para o depósito PCT)
- `docs/` com 6 documentos principais (README, ARCHITECTURE, PROTOCOL,
  CRYPTO, THREAT_MODEL, PCT_CLAIMS_MAPPING, GLOSSARY) + 10 ADRs
- Sequence diagrams em Mermaid embarcados nos `.md` (renderizam em
  GitHub/GitLab sem ferramenta externa)
- `PCT_CLAIMS_MAPPING.md` lista cada reivindicação técnica → linha
  de código que a implementa → teste que a valida
- `THREAT_MODEL.md` enumera 16 superfícies de ataque com status de
  cobertura e próximos passos
- `CRYPTO.md` formaliza derivação, codificação base32, encadeamento
  C₁→C₂ e propriedades de segurança
- 10 ADRs documentam decisões: HMAC com domain separation, alfabeto
  restrito do SeedId, comparação em tempo constante, núcleo zero-I/O,
  Ed25519 com pub key embarcada, separação FFI/UniFFI, modos duais de
  T, identidade via SAN URI, subset no_std, política de não-pânico

**Sessão 9** (port para microcontroladores — `no_std` sem alloc)
- Nova crate `mcpix-embed`: `#![no_std]`, sem allocator, sem rede.
  Subset receiver-only que cabe em ESP8266 / ESP32-C3 / Cortex-M4.
- API mínima: `derive_pair(&seed, counter) -> (C1, C2)`,
  `encode_into(&sid, &c1, &mut [u8; 35]) -> &str`,
  `charge_qr(field, &mut tmp, &mut out) -> QrCode<'_>` (feature `qr`).
- Cross-validação em `tests/cross_validate.rs` confirma byte-a-byte
  igualdade com `mcpix-core` para conjunto amostral — algoritmo idêntico
  no host e no firmware.
- `embedded/`: projeto bare-metal `cortex-m-rt` (excluído do workspace
  principal) com `panic-halt`. Build local validado:
  - `thumbv7em-none-eabihf` (Cortex-M4F) ✓
  - `riscv32imc-unknown-none-elf` (ESP32-C3) ✓
  - Binário demo completo (derive_pair + encode + QR + iteração): 16.5 KB
    `.text`, 0 RAM estática, 0 BSS.
- 7 testes (4 unit + 3 cross-validation contra `mcpix-core`).
- Para ESP8266 Xtensa: SDK porta tal qual; ponto crítico é a toolchain
  (fork Espressif via `espup install`).

## Backlog

Curadoria do que está explicitamente diferido (referência: `docs/THREAT_MODEL.md` §9,
parágrafos "fora do escopo" pelos docs e resumos de sessão).

### Threat model gaps (não cobertos)

1. **LD_PRELOAD / DLL hijacking** (THREAT_MODEL §5.4) — mitigação via
   remote attestation / TEE (Secure Enclave iOS, StrongBox Android, TPM
   desktop). Depende de capacidade da plataforma; não é SDK puro.

### Contratos prontos, integração hw real pendente

2. **`SeedSealer` para Secure Element** (§7.1) — trait + `SealedInMemorySeedStore`
   + mock `ChaChaSealer` + 7 testes de invariante prontos. Falta plugar
   iOS Secure Enclave, Android StrongBox, TPM 2.0 — esqueletos em
   `docs/SECURE_ELEMENT.md`. Exige device físico para validar.

### Resíduos (cobertos, com parcela diferida)

3. **SLSA L4 hermético** (§5.3 / `docs/SLSA.md` / `docs/SLSA_L4_PROGRESS.md`) —
   `xtask verify-hermetic` + toolchain pinada + catálogo de `build.rs`
   + `reproducibility.yml` (cross-runner hash compare via `workflow_dispatch`)
   prontos. Falta: CI rotineiro do hermetic build no `release.yml`,
   schedule periódico do `reproducibility.yml`, audit crate-a-crate
   da Categoria C, e tarball-determinismo para AAR/NuGet.
4. **Live OCSP query (Phase 1)** (§6.5 / `docs/MTLS_REVOCATION.md`) —
   request builder + parser + transport + 9 testes prontos. Falta
   Phase 2: verificação criptográfica da assinatura da OcspResponse
   contra a CA (hoje delegada ao integrator).
5. **Anti-replay de backup** (`mcpix-backup/src/lib.rs`) — protocolo
   institucional para invalidar backup antigo no banco recebedor antes
   de aceitar device novo.

### Qualidade / DX

6. **Capability negotiation inter-bancos** — `BankReceiver::supported_versions()`
   exposto via API HTTP; infra do `ProtocolVersion::all()` pronta desde S16.
7. **Corpus de fuzz versionado** — `fuzz.yml` roda mas corpus
   regressivo não está em git; achados anteriores deveriam virar testes.
8. **Testes instrumented Android (Espresso) + iOS (XCUITest)** —
   exige app integrador real para ter ROI.
9. **OID privado para SAN URI** (ADR 0008) — alternativa formal a
   `urn:mcpix:institution:`.
10. **Demo web — LocalStorage persistência** (modo quantizado já
    entregue em S23). Requer iteração sobre `InMemorySeedStore` que
    o trait `SeedStore` não expõe hoje — extensão da trait + import/
    export JSON.

