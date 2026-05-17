# ADR-0006: Crates FFI e UniFFI separados

## Status

Aceito — implementado em S2.

## Contexto

A especificação (Bloco 2) prescreve **UniFFI** para Swift/Kotlin e
**assinaturas C nativas** para o ecossistema .NET (P/Invoke). Duas
abordagens com bibliotecas runtime distintas.

Misturar ambas no mesmo `cdylib` é tecnicamente possível: UniFFI gera
suas próprias funções `extern "C"` (prefixadas `uniffi_*`) ao lado
das `extern "C"` manuais (`mcpix_*`). Mas:

- Tabela de símbolos exportados fica poluída — auditor encontra
  ~50 símbolos `uniffi_*` ao lado dos 5 que `.NET` consome.
- Versão de `uniffi` precisa casar com o `uniffi-bindgen` que gera
  os bindings Swift/Kotlin. Mudanças de versão impactam o `cdylib`
  consumido por .NET sem que .NET tenha visibilidade.

## Decisão

Manter dois crates separados produzindo dois `cdylib` distintos:

- **`mcpix-ffi`**: 5 funções `extern "C"` manuais + header gerado
  por `cbindgen`. Consumido por .NET via `DllImport`.
- **`mcpix-uniffi`**: anotações `#[uniffi::export]` em fachada Rust
  específica + scaffolding gerado por `uniffi::setup_scaffolding!()`.
  Consumido por Swift e Kotlin via UniFFI.

Workspace tem dois pacotes que produzem `libmcpix_ffi.{so,dll,dylib}`
e `libmcpix_uniffi.{so,dll,dylib}` independentes.

## Alternativas consideradas

### A1. Crate único `mcpix-ffi-all` com ambas as superfícies

```toml
[features]
default = []
uniffi = ["dep:uniffi"]
cbindgen = ["dep:cbindgen"]
```

**Por que não.** Cargo features são aditivas — `cargo build` de
qualquer caller que toque o crate vai sempre ativar todas as features
que algum membro do workspace requer. Resultado: o `cdylib` final
sempre tem ambos scaffoldings, anulando o objetivo.

### A2. Só UniFFI (incluindo para .NET)

UniFFI tem suporte experimental para C# via terceiros
(`uniffi-bindgen-cs`).

**Por que não.** Imaturo em 2026. P/Invoke é o padrão idiomático
.NET — usar P/Invoke direto é mais auditável para integradores .NET
acostumados ao padrão. Ganho de unificação é teórico.

### A3. Só C-ABI manual (sem UniFFI)

Escrever wrappers Swift e Kotlin à mão sobre `mcpix-ffi`.

**Por que não.** UniFFI gera tipos idiomáticos (Object com ARC em
Swift, AutoCloseable em Kotlin, Records, Enums, Exceptions) que
seriam custosos de manter à mão e propensos a drift. Aceitamos a
duplicação de cdylib para ganhar essa qualidade.

## Consequências

**Positivas:**

- Tabela de símbolos de cada `.so` é limpa: `nm libmcpix_ffi.so |
  grep -c 'T mcpix_'` retorna 5 (`new`, `register`,
  `generate_charge`, `validate`, `free` + `string_free`).
- Versão de `uniffi` pode evoluir sem impactar consumidores .NET.
- Auditoria de cada superfície é independente.

**Negativas:**

- Dois binários nativos por plataforma → CI faz dois builds, cada
  artefato é hash-ado e assinado separadamente. Custo de pipeline
  marginal.
- Tamanho total da distribuição dobra para plataformas que
  consumiriam ambos — irrelevante na prática porque cada plataforma
  consome só um.

## Validação

- `mcpix-ffi/src/receiver_api.rs::tests::ffi_full_flow` — fluxo
  completo via C-ABI manual.
- `mcpix-uniffi/src/lib.rs::tests` — round-trip via UniFFI.
- `bindings/kotlin/src/test/kotlin/SmokeTest.kt` — Kotlin JVM
  carregando `libmcpix_uniffi.so` via JNA, exercitando
  `register/generateCharge/validateReceipt`.

## Referências

- Bloco 2.1 e Bloco 3 da especificação técnica.
- Mozilla UniFFI [docs](https://mozilla.github.io/uniffi-rs/).
- `cbindgen` [docs](https://github.com/mozilla/cbindgen).
