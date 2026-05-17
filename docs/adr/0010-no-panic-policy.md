# ADR-0010: Política de não-pânico cobrindo FFI com `catch_unwind`

## Status

Aceito — implementado em S1, propriedades adicionadas em S5.

## Contexto

A especificação técnica (Bloco 1.3) é categórica:

> O SDK **nunca** deve capotar (*panic*) a aplicação hospedeira.
> Todas as funções públicas expostas na FFI devem ser protegidas por
> `std::panic::catch_unwind`.

Implicações:

- Erros internos do núcleo devem retornar via `Result<_, McpixError>`,
  não via panic.
- Panics que escapem (bugs, asserts deixados acidentalmente) devem
  ser absorvidos na fronteira FFI e convertidos em código de erro.
- Bindings nativos (Swift, Kotlin, .NET) **não podem** crashar o
  processo hospedeiro porque a SDK falhou.

## Decisão

**Camada Rust** (`mcpix-core`, `mcpix-receiver-sdk`, etc.):

- Toda função pública retorna `Result<_, McpixError>`.
- Nenhum `unwrap()`/`expect()` em código de produção — apenas em
  testes ou em construtores de constantes onde a invariante é
  provada por construção (e o código é coberto por testes).
- Comparações de tamanho de array via `try_into()` (que retorna
  `Result`), não `expect` arbitrário.

**Camada FFI** (`mcpix-ffi`):

- Toda função `extern "C"` envolve seu corpo em `catch_unwind`.
- Helper `guard`/`guard_mut` em `handle.rs` faz isso uma vez:

```rust
pub(crate) fn guard_mut<F>(f: F) -> McpixStatus
where F: FnOnce() -> McpixStatus,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(s) => s,
        Err(_) => McpixStatus::Panic,
    }
}
```

- `McpixStatus::Panic = 98` é distinguível dos demais códigos de
  erro do protocolo → caller nativo pode logar e tomar ação
  específica (ex. reiniciar SDK).

**Camada UniFFI** (`mcpix-uniffi`):

- UniFFI gera scaffolding que captura panics automaticamente e os
  transforma em `UniffiInternalError` no lado nativo (Swift /
  Kotlin). Garantido pelo framework — não precisamos repetir
  `catch_unwind` manualmente.

## Alternativas consideradas

### A1. `panic = "abort"` em todos os builds

**Por que não.** Garantia binária: panic mata o processo. Não
queremos isso em release porque mata o **app hospedeiro**, não só
o SDK. Em release usamos `panic = "abort"` apenas para o cdylib
quando faz sentido (cdylib não retorna unwind para o caller —
panics não podem cruzar fronteira FFI mesmo com unwind). A
**política** de não-pânico é semântica, não apenas binária.

### A2. `panic = "unwind"` sem `catch_unwind`

**Por que não.** Unwind cruzando FFI é UB (undefined behavior). O
behavior varia entre alvos: em alguns, simplesmente aborta; em
outros, corrompe stack do caller. `catch_unwind` é obrigatório.

### A3. Result types sem variant `Panic`

Tratar panics como erro genérico.

**Por que não.** Caller nativo precisa distinguir "erro esperado do
protocolo" (ex. `Mismatch`) de "bug interno do SDK". Variant
`Panic` deixa isso explícito.

## Consequências

**Positivas:**

- App hospedeiro nunca crasha por bug da SDK. Auditoria de
  segurança fica mais leve — o blast radius é limitado.
- Panics que escapem são observáveis via `McpixStatus::Panic` em
  logs do caller — facilita debug em produção.
- Property tests + fuzz têm critério de sucesso claro: "qualquer
  input deve retornar `Result`, nunca panicar".

**Negativas:**

- Pequena overhead de `catch_unwind` em cada chamada FFI
  (~10ns + setjmp/longjmp). Irrelevante para o uso (chamadas de
  protocolo são raras).
- `AssertUnwindSafe` é usado nos handlers — exige verificar
  manualmente que o callable não deixa estado inconsistente em
  panic. Mitigado pelo padrão "computa local + commita no fim".

## Validação

| Propriedade | Mecanismo |
|---|---|
| Parser nunca panica em input arbitrário | `properties::parse_never_panics_on_arbitrary_strings` (proptest, 256+ casos) |
| `verify_combined` nunca panica | `properties::verify_combined_never_panics` |
| Inputs adversariais via libfuzzer | `fuzz/fuzz_targets/{fuzz_transport_parse,fuzz_sums_line,fuzz_verify_combined}.rs` — 25M+ inputs sem crash |
| FFI propaga panic como `McpixStatus::Panic` | Cobertura manual — não testado automatizado porque exige forçar panic deliberado |

## Auditoria recomendada

Comando para verificar não-uso de `unwrap`/`expect` fora de testes:

```bash
rg '\.unwrap\(\)|\.expect\(' crates/ --type rust --glob '!**/tests/**' --glob '!**/*_test*.rs'
```

Hoje retornam ocorrências esperadas: construtores de constantes
(`expect("ASCII by construction")` em `as_str()` onde a string foi
validada na construção), e expects em testes.

## Referências

- Especificação técnica, Bloco 1.3.
- Rust Nomicon, "Unwinding" — semântica de panic cruzando FFI.
- Rust RFC 1513 (panic strategy) — abort vs unwind.
