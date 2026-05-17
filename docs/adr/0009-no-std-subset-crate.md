# ADR-0009: Subset `no_std` em crate separada para microcontroladores

## Status

Aceito — implementado em S9.

## Contexto

Cenários operacionais incluem dispositivos de baixo recurso atuando
como recebedor: terminais de pagamento simples, vending machines,
totens com display e-paper, dispositivos NFC autônomos. Hardware
típico: ESP8266 (~50 KB RAM), ESP32-C3 (~320 KB RAM), Cortex-M4F.

Restrições destes ambientes:

- Sem `std` library, sem `alloc` (sem heap dinâmico).
- Footprint de código mede em dezenas de KB, não em MB.
- Sem rede em muitos casos (apenas emissão do QR).
- Sem TLS embarcado em ESP8266 (RAM insuficiente).

A SDK host (`mcpix-core`) usa `String`/`Vec` em alguns tipos
(`SeedId`, `Charge::transport_field`), bloqueando build direto para
target embarcado.

## Decisão

Criar crate **separada** `mcpix-embed` com `#![no_std]` e zero
allocator. Subset deliberadamente receiver-only — emite cobrança,
não valida (validação exige store persistente fora do escopo MCU
nesta entrega).

Algoritmo **bit-exato** com `mcpix-core`. Cross-validation em
`tests/cross_validate.rs` (host test que importa ambas as crates)
garante não-drift.

Estrutura:

```
mcpix-embed/
├── src/
│   ├── lib.rs                # #![no_std]
│   ├── crypto.rs             # derive_pair, derive_c2_from_c1, verify_c2
│   ├── transport_field.rs    # encode_into(&mut [u8; 35]) → &str
│   ├── types.rs              # SeedId em heapless::Vec, C1/C2 em [u8; 11]
│   └── qr.rs                 # feature "qr" — qrcodegen-no-heap wrapper
└── tests/
    └── cross_validate.rs     # std test contra mcpix-core
```

## Alternativas consideradas

### A1. `mcpix-core` ganha feature `std` (default on)

Single source of truth com flag para no_std.

**Por que não.** A maior parte do `mcpix-core` usa `String`/`Vec` em
mais de uma dezena de pontos. `cfg`-gating cada um faria a leitura
do código mais difícil para revisor cripto. Crate separada mantém o
diff de leitura focado.

### A2. Reusar `mcpix-core` via `default-features = false`

**Por que não.** Mesma razão acima — mais a complicação de
`thiserror` em `no_std` (precisa feature `no_std` que tinha API
diferente até versão recente).

### A3. Suporte `alloc` (heap embarcado)

Manter API atual mas usar allocator do `esp-idf-rs`.

**Por que não.** Heap embarcado pode fragmentar em devices de vida
longa (vending machine ligada 24/7 por anos). Sem alloc dá garantia
de footprint constante, comportamento determinístico, e é a prática
recomendada para dispositivos de pagamento.

### A4. Reescrita em C

Implementar uma versão MCU-ready em C, mantida em paralelo.

**Por que não.** Duplicação de código em duas linguagens é o cenário
de bug mais difícil de detectar. Rust `no_std` cumpre o requisito
sem essa dívida.

## Consequências

**Positivas:**

- Algoritmo idêntico ao host validado a cada build de CI
  (`cargo test -p mcpix-embed`).
- Binário Cortex-M4F do `embedded/` demo: 16,572 bytes `.text`,
  0 BSS — cabe em qualquer MCU alvo, incluindo ESP8266.
- Sem allocator → footprint previsível, sem fragmentação de heap.
- `forbid(unsafe_code)` na crate é honesto: usa apenas APIs safe.

**Negativas:**

- Duplicação de código (~150 linhas em `crypto.rs`,
  `transport_field.rs`, `types.rs`). Mitigada pelo cross-validate
  que falha imediatamente em drift.
- Para ESP8266 (Xtensa LX106): SDK porta tal qual, mas toolchain Rust
  exige fork da Espressif (`espup install`). Documentado.

## Footprint medido

```
$ arm-none-eabi-size embedded/target/thumbv7em-none-eabihf/release/mcpix-embed-demo
   text    data     bss     dec     hex
  16572       0       0   16572    40bc
```

Detalhe do que cabe em 16.5 KB:

- `derive_pair` + `derive_c2_from_c1` + `verify_c2` (~2 KB)
- HMAC-SHA-256 backend (~3 KB)
- Encoder base32 custom + transport field (~500 bytes)
- `qrcodegen-no-heap` (~7 KB)
- Cortex-M runtime + panic-halt (~3 KB)

## Validação

| Propriedade | Teste |
|---|---|
| `derive_pair` igual ao host para conjunto amostral | `cross_validate::derive_pair_matches_core_for_sampled_inputs` |
| `encode_into` igual ao host | `cross_validate::encode_field_matches_core` |
| Round-trip encode/parse | `transport_field::tests::encode_then_parse_roundtrip` |
| Determinismo bare-metal | `crypto::tests::deterministic_on_target` |
| QR module grid não-vazio | `qr::tests::charge_produces_decodable_qr_module_grid` |

## Targets validados

- `thumbv7em-none-eabihf` (Cortex-M4F) — compila release ✓
- `riscv32imc-unknown-none-elf` (ESP32-C3 class) — compila release ✓
- `xtensa-esp8266-none-elf` — requer toolchain fork Espressif;
  validação em hardware real fora do escopo desta entrega.

## Referências

- Especificação técnica, Bloco 4.1 (alvos de compilação).
- [embedded.rs book](https://docs.rust-embedded.org/book/) —
  guia de práticas para `no_std`.
- [`heapless`](https://crates.io/crates/heapless) — estruturas de
  dados de tamanho fixo para Rust embarcado.
