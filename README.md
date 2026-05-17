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
# testes Rust (default + sqlite + integração contra dist/)
cargo test --workspace
cargo test --workspace --features mcpix-receiver-sdk/sqlite
cargo test --workspace -- --include-ignored

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
```

## Self-check de integridade

O núcleo (`mcpix_core::integrity`) e o glue runtime (`mcpix_receiver_sdk::integrity_runtime`)
implementam verificação SHA-256 do binário carregado:

- Em build de release o pipeline injeta `MCPIX_EXPECTED_SHA256` ⇒ `verify_self()` retorna
  `Verified` quando o `.so`/`.dylib`/`.dll` carregado bate com o hash do release,
  `Tampered { expected, actual }` se houve adulteração.
- Em build dev (sem env var carimbada) o método retorna `Skipped` para não atrapalhar
  desenvolvimento.
- Caller (fachada Swift/Kotlin/.NET) deve abortar inicialização em `Tampered`.

Cobertura: 5 testes unitários em `crypto`/`integrity` + 2 integration tests
contra o `.so` real (rodam após `cargo xtask build-linux`).

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

## Próximas sessões

1. Assinatura GPG/cosign dos artefatos + verificação de cadeia na inicialização
2. Remote attestation/TEE para defesa contra LD_PRELOAD e DLL hijacking
3. Testes integrados Android (instrumented) e iOS (XCUITest)
