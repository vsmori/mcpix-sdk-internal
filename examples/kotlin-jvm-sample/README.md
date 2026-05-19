# Sample Kotlin JVM — CLI consumindo UniFFI

App console (não-Android) que exercita o binding Kotlin gerado por
UniFFI. Útil para validar a integração JVM sem cargo do build
Android. Para o app Android completo (com Activity + UI), ver
[`examples/android-sample/`](../android-sample/).

## Pré-requisitos

- JDK 21+
- `libmcpix_uniffi.so` em `target/debug/` ou `target/release/`:
  ```bash
  cargo build -p mcpix-uniffi
  ```

## Build & run

```bash
gradle run                               # usa target/debug por default
gradle run -Pcdylib.dir=/abs/path/lib    # ou aponte para outro diretório
```

Saída esperada:

```
=== mcpix-sdk — demo integrador Kotlin (JVM) ===

✓ recebedor cadastrado: SeedId=RECVR1
✓ cobrança gerada:
    transport field (público): PIXOFFv1RECVR10000000000XXXXXXXXXXX
    counter T:                  1
    layout: 8 (prefix) + 16 (SeedId padded) + 11 (C₁) = 35 chars

✓ validação com C₂ errado:
    outcome: MISMATCH  (esperado: MISMATCH — defesa anti-tampering)
```

## Por que JNA e não JNI

UniFFI gera bindings que carregam o `.so` via JNA — biblioteca Java de
FFI baseada em ABI. Vantagem: zero código nativo manual. JNI puro
exigiria stubs `extern "C"` em Rust + headers Java gerados, processo
mais sensível. A SDK escolheu JNA via UniFFI para que o caminho
Android (ver `examples/android-sample/`) e este JVM samples
compartilhem o mesmo binding.

## O que o sample não cobre

Mesmo conjunto do dotnet-sample: C₂ válido (que exige a face do
banco pagador), persistência cross-restart, e mTLS inter-bancos —
todos em [`../e2e_demo.rs`](../e2e_demo.rs).
