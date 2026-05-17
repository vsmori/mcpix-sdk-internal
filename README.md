# mcpix-sdk-internal

SDK Rust de validação local de transações de pagamento. Pré-depósito PCT — uso interno.

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
# testes Rust (default + sqlite + integração contra dist/ + propriedades)
cargo test --workspace
cargo test --workspace --features mcpix-receiver-sdk/sqlite
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

## Próximas sessões

1. Remote attestation/TEE para defesa contra LD_PRELOAD e DLL hijacking
2. SLSA L3+ build provenance (sigstore + transparency log)
3. Testes integrados Android (instrumented) e iOS (XCUITest)
