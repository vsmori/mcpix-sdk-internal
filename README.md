# mcpix-sdk-internal

SDK Rust de validação local de transações de pagamento. Pré-depósito PCT — uso interno.

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

## Próximas sessões

1. Remote attestation/TEE para defesa contra LD_PRELOAD e DLL hijacking
2. SLSA L3+ build provenance (sigstore + transparency log)
3. Testes integrados Android (instrumented) e iOS (XCUITest)
