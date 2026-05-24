# Sample embedded — bare-metal Cortex-M4F

O demo embedded **já existe** em [`embedded/`](../../embedded/) no
root do repo (não pôde ficar em `examples/` porque cargo workspaces
não suportam misturar bare-metal e host crates no mesmo membro —
`embedded/` está explicitamente em `exclude` do workspace top-level).

Este README aponta para lá e descreve o que o demo cobre, para que
integradores embarcados encontrem o ponto de partida correto.

## O que está lá

[`embedded/src/main.rs`](../../embedded/src/main.rs) — ~200 linhas
de Rust `no_std` exercitando:

1. Geração de `Seed` (estática no demo; em produção viria de
   Secure Element / eFuse).
2. Carga do `last_t` persistido no `CounterStore` (flash).
3. Avanço de T com **anti-rollback de energia** — persiste em flash
   antes de derivar `(C₁, C₂)`.
4. Derivação `(C₁, C₂)` via `mcpix-embed::crypto::derive_pair`.
5. Encode do campo de transporte público.
6. Geração de QR Code via `qrcodegen-no-heap`.
7. Persistência do `C₂` retido no `ReceiptStore`.
8. **Simulação de reboot** (drop + reopen dos stores sobre o mesmo
   flash) — verifica que `load()` recupera retained + counter
   idênticos.
9. Marca `consumed` (cobranças posteriores não revalidam).

Status reportado por debugger (`status` AtomicU8):

```
0   = pronto
1   = derivação OK
2   = encode OK
3   = QR OK
4   = persist OK
5   = reboot+load OK
6   = consumed marked
0xFF = erro
```

## Build

```bash
rustup target add thumbv7em-none-eabihf
cd embedded
cargo build --release --target thumbv7em-none-eabihf
# Tamanho do binário:
arm-none-eabi-size target/thumbv7em-none-eabihf/release/mcpix-embed-demo
```

CI valida tamanho em [`ci.yml`](../../.github/workflows/ci.yml) —
guarda contra regressão de footprint.

## Porting para outros MCUs

O demo usa `RamFlash` (impl in-process de `embedded-storage::NorFlash`)
para que o teste rode no host sem hardware. Em produção:

| Família | Backend |
|---|---|
| ESP32-C3 / S3 / H2 | [`esp-storage`](https://crates.io/crates/esp-storage) |
| STM32 (todos) | `stm32xx_hal::flash` (varia por subfamília) |
| nRF52 / nRF53 | `nrf-hal::nvmc` |
| RP2040 / RP2350 | `rp2040-flash` |
| ESP8266 (Xtensa LX106) | porta com toolchain Espressif (`espup`) |

Substituir `RamFlash` por uma dessas é trocar a tipagem genérica
`F: NorFlash` no `ReceiptStore<F>` / `CounterStore<F>` — zero
mudança no resto do código.

## Tamanho típico

Stripped, release, LTO + `opt-level = "z"`:

| Componente | Tamanho |
|---|---|
| `mcpix-embed` core (HMAC + types + transport) | ~12 KB |
| `mcpix-embed` com `qr` feature | ~22 KB |
| `mcpix-embed` com `qr + storage` | ~32 KB |
| `mcpix-embed` com `qr + storage + restore` (Argon2 + ChaCha20) | ~115 KB |

`restore` (recuperação de backup criptografado) é a única feature
que infla footprint significativamente — Argon2 + ChaCha20-Poly1305
+ bs58 somam código. Habilite só se o fluxo de restore vive no MCU
(alternativa: restore acontece num companion app móvel via
`mcpix-backup` no host, e o MCU recebe só a Seed em claro via
canal seguro).

## Limitações conhecidas

- **Counter persistido em RAM no demo** — o `RamFlash` é volátil. Em
  hardware real, o `NorFlash` da plataforma preserva entre boots.
- **Sem validação no MCU** — `mcpix-embed` não expõe
  `apply_validate_receipt` porque exige `RetainedReceipt` carregado
  (que existe; o storage faz). Adicionar é trivial mas fora deste
  demo.
- **ESP8266**: porta funcional mas Argon2 com m=64 KiB ocupa ~1/3 da
  RAM (50 KB total) — restore é justo no limite. Evite combinar com
  outros consumidores de RAM grandes na mesma image.
