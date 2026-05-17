# mcpix-sdk-internal

SDK Rust de validação local de transações de pagamento. Pré-depósito PCT — uso interno.

## Estrutura

```
crates/
  mcpix-core              núcleo criptográfico, sem I/O
  mcpix-receiver-sdk      SDK do recebedor (compõe núcleo + stores locais)
  mcpix-bank-receiver     módulo backend do banco recebedor (custódia de sementes)
  mcpix-bank-payer-mock   simulação do banco do pagador (substituição institucional)
  mcpix-ffi               camada C-ABI (Swift/Kotlin/.NET via UniFFI/P-Invoke)
examples/
  e2e_demo.rs             demo CLI dos 3 módulos lado a lado
```

## Executar

```bash
cargo test --workspace
cargo test --workspace --features mcpix-receiver-sdk/sqlite

cargo run -p mcpix-examples --bin e2e_demo
```

## Cobertura de testes (sessão 1)

- determinismo `(S, T) → (C₁, C₂)`
- validação positiva e negativa de `C₂`
- defesa de replay (segunda apresentação rejeitada)
- comparação em tempo constante via `subtle::ConstantTimeEq`
- parse/encode do campo de transporte (`PIXOFFv1` + SeedId-16 + C₁-11 = 35 chars)
- round-trip do `SqliteSeedStore` (feature `sqlite`)
- fluxo completo via FFI C-ABI (`mcpix_receiver_*`)

## Próximas sessões

1. Bindings UniFFI (Swift / Kotlin) + headers C gerados
2. Pipeline cross-compile para `aarch64-apple-ios`, `aarch64-linux-android`, etc.
3. Assinatura digital de artefatos + verificação de integridade na inicialização
