# ADR-0004: Núcleo isolado de I/O via traits injetadas

## Status

Aceito — princípio fundacional da arquitetura, S1.

## Contexto

A especificação técnica (Bloco 1.2) exige que o "Core não pode
instanciar clientes HTTP, ler arquivos do disco ou acessar chaves de
hardware diretamente. Ele define contratos (Traits) que as plataformas
nativas devem implementar e injetar."

Razões subjacentes:

- **Auditabilidade**: revisor cripto não deve precisar raciocinar
  sobre concorrência de I/O para verificar correção do protocolo.
- **Portabilidade**: o mesmo núcleo precisa rodar em Rust host (std),
  bare-metal MCU (no_std), e dentro de bindings que cruzam threading
  boundaries (Swift `async`, Kotlin Coroutines).
- **Substituição futura**: o `SeedStore` deve poder migrar para
  Secure Enclave / HSM sem mudança no núcleo.

## Decisão

`mcpix-core` declara traits e tipos puros. **Nenhuma** linha do
núcleo abre arquivo, faz syscall de rede, ou consulta o relógio do
sistema diretamente. Toda dependência externa entra via injeção de
trait:

| Trait | Responsabilidade |
|---|---|
| `SeedStore` | Persistência de seeds e retained receipts |
| `Counter` | Geração de `T` (sequencial ou quantizado) |
| `SecureRandom` | CSPRNG da plataforma |
| `Clock` | Relógio injetável (testável, substituível por TEE) |
| `HttpTransport` | Stub para transporte plataforma-injetado |

Implementações concretas vivem nas **fachadas**: `mcpix-receiver-sdk`,
`mcpix-bank-receiver`, etc.

## Alternativas consideradas

### A1. Núcleo monolítico com I/O direto

Usar `std::fs`, `reqwest`, `rusqlite` diretamente no núcleo.

**Por que não.** Quebra portabilidade para `no_std`. Acopla o núcleo
a versões específicas de libs IO. Auditoria cripto fica misturada
com auditoria de IO.

### A2. Traits com `async fn`

```rust
pub trait SeedStore { async fn put_seed(...); }
```

**Por que não.** UniFFI ainda tem suporte limitado a traits async
em superfícies expostas. C-ABI manual (`mcpix-ffi`) não tem
representação natural de async. Pagaríamos complexidade em todas
as fachadas para benefício marginal — store local é I/O rápido.

### A3. Closures em vez de traits

Passar `Box<dyn Fn(...)>` em vez de `Arc<dyn SeedStore>`.

**Por que não.** Closures não compõem bem em FFI (não atravessam
fronteiras). Traits permitem objetos com estado, drop semantics, e
são naturais em todos os bindings (Swift protocol, Kotlin
interface, .NET interface).

## Consequências

**Positivas:**

- Cross-compile para `thumbv7em-none-eabihf` e
  `riscv32imc-unknown-none-elf` funciona com mudança cosmética: o
  núcleo já é I/O-free; `mcpix-embed` só precisa de tipos no_std
  para coisas como `String → heapless::String`.
- Testes do núcleo usam mocks triviais (in-memory) — não há sequer
  necessidade de tokio para testar a lógica de protocolo.
- Auditoria cripto tem perímetro definido: `crates/mcpix-core/src/`
  é o universo de revisão.
- Persistência futura em HSM/Secure Element: nova impl de
  `SeedStore` em `mcpix-receiver-sdk` ou crate nova; núcleo
  permanece intocado.

**Negativas:**

- Aumenta verbosidade em call sites: caller passa `Arc<dyn ...>` em
  vez de chamar diretamente. Mitigado pelas fachadas que constroem
  defaults sensíveis (`ReceiverSdk::new()`).
- Pequena indireção dinâmica: `Arc<dyn SeedStore>` é dispatch
  virtual. Custo na ordem de 5ns por chamada — irrelevante para o
  uso (geração ocorre poucas vezes por segundo no recebedor).

## Validação

- `mcpix-core` não tem dependência de `std::fs`, `tokio`, `reqwest`
  ou similares — verificável por `cargo tree -p mcpix-core`.
- `mcpix-embed` reusa as primitivas do núcleo em `no_std` sem
  modificação — `crates/mcpix-embed/src/crypto.rs` aplica
  literalmente o mesmo algoritmo.
- Cross-validation `crates/mcpix-embed/tests/cross_validate.rs`
  confirma resultados bit-exatos.

## Referências

- "Hexagonal Architecture" (Alistair Cockburn, 2005) — fonte
  conceitual da separação ports / adapters.
- Bloco 1.2 da especificação técnica (`ESPECIFICAÇÃO TÉCNICA_*.md`).
