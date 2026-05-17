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
# testes Rust
cargo test --workspace
cargo test --workspace --features mcpix-receiver-sdk/sqlite

# demo CLI end-to-end
cargo run -p mcpix-examples --bin e2e_demo

# regenerar bindings Swift/Kotlin/.h
cargo xtask gen-bindings
cargo xtask check-bindings        # CI: falha se houver drift

# smoke test do binding Kotlin contra o cdylib (libmcpix_uniffi.so)
cargo build -p mcpix-uniffi
(cd bindings/kotlin && gradle test)
```

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

## Próximas sessões

1. Pipeline cross-compile para `aarch64-apple-ios`, `aarch64-linux-android`,
   `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc` + empacotamento
   XCFramework / AAR / NuGet
2. Assinatura digital de artefatos + verificação de integridade na inicialização
