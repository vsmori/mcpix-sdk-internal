# Sample Arduino UNO Q — STM32U585 (Cortex-M33), MCU side

A [Arduino UNO Q](https://store.arduino.cc/products/uno-q) é uma placa
**dual-arquitetura**:

- **STM32U585** (Cortex-M33, ARMv8-M Main, TrustZone-capable) — tempo
  real, I/O, AES on-chip, secure storage via TrustZone.
- **Qualcomm Dragonwing** (ARM Cortex-A, roda Debian) — UI,
  conectividade, ecossistema de Bricks (containers do app store).

Existem dois lugares onde a SDK pode rodar nessa placa. **Este sample
cobre o lado MCU** — o caminho com menor superfície de ataque, sem SO
de propósito geral abaixo do crypto. Veja [Trade-off](#trade-off-mcu-vs-linux-side)
no fim deste documento.

## Onde está o código

O sample bare-metal está em
[`embedded-uno-q/`](../../embedded-uno-q/) no root do repo — não em
`examples/` porque cargo workspaces não suportam misturar bare-metal e
host crates no mesmo workspace (`embedded-uno-q/` está explicitamente
em `exclude` do workspace top-level, mesmo motivo de `embedded/`).

Este README aponta para lá e descreve o que o demo cobre.

## O que o demo cobre

[`embedded-uno-q/src/main.rs`](../../embedded-uno-q/src/main.rs)
exercita o mesmo fluxo do demo embedded genérico, com hooks no comentário
para o que muda em produção no U585:

1. Geração de `Seed` (estática no demo; em produção, derivada via
   TrustZone-protected key store do U585).
2. Carga do `last_t` persistido no `CounterStore` (flash).
3. Avanço de T com **anti-rollback de energia** — persiste em flash
   antes de derivar `(C₁, C₂)`. Crítico em IoT: o UNO Q pode ser
   desconectado a qualquer momento pelo usuário final.
4. Derivação `(C₁, C₂)` via `mcpix-embed::crypto::derive_pair`.
5. Encode do campo de transporte público (35 chars).
6. Geração de QR Code via `qrcodegen-no-heap` (~5ms — apresentado no
   display do lado Dragonwing via SPI).
7. Persistência do `C₂` retido no `ReceiptStore`.
8. **Simulação de reboot** (drop + reopen dos stores) — verifica que
   `load()` recupera retained + counter idênticos.
9. Validação local em tempo constante via `verify_c2` e marcação
   `consumed` em flash — defesa de replay sobrevive a reboot.

Status reportado por debugger (`STATUS` AtomicU8):

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
cd embedded-uno-q/
rustup target add thumbv8m.main-none-eabihf
cargo build --release
```

O `.cargo/config.toml` já define `thumbv8m.main-none-eabihf` como
target default, então `cargo build` daqui já cross-compila.

O binário sai em
`target/thumbv8m.main-none-eabihf/release/mcpix-uno-q-demo`.

Para medir o tamanho:

```bash
arm-none-eabi-size target/thumbv8m.main-none-eabihf/release/mcpix-uno-q-demo
```

## Layout de memória

`embedded-uno-q/memory.x` reflete o STM32U585:

```
FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 2048K
RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 256K
```

Usa SRAM1+SRAM2 contíguos (192K + 64K). O U585 tem mais SRAM
(SRAM3/SRAM4) em endereços não-contíguos — se precisar, dá para mapear
via `MEMORY` regions adicionais e usar `#[link_section]`.

## Trade-off: MCU side vs Linux side

| Aspecto | **MCU side** (este sample) | Linux side (Dragonwing) |
|---|---|---|
| Crate principal | `mcpix-embed` (`no_std`) | `mcpix-receiver-sdk` (std, libstd) |
| Superfície de ataque | Mínima — sem kernel, sem userspace | Toda a stack Debian + Brick container |
| Custódia da Seed | TrustZone / AES on-chip | Filesystem ou Keychain (proteção lógica) |
| Persistência | Flash do MCU via `NorFlash` trait | SQLite ou JSON em disk do Dragonwing |
| Conectividade | Nenhuma — vai pelo Dragonwing | Direta (Wi-Fi/eth) |
| Cripto rapidez | AES on-chip (hardware) | OpenSSL/RustCrypto (software) |
| UX dev | Cross-compile + flash via debugger | `apt install`/Brick build |

**Quando ir de MCU side:** POS de balcão, kiosk de pagamento, qualquer
caso onde a propriedade "C₂ nunca atravessa um SO geral" tem valor de
defesa (auditoria, certificação PCI, threat model que assume Linux
comprometido).

**Quando ir de Linux side:** prototipagem rápida, casos onde o material
da Seed já está em backup criptografado vindo do servidor, ou onde a
UI Dragonwing é o produto principal.

A versão Linux-side é trivial: instala libs, instancia
`McpixReceiver`, faz P/Invoke ou usa o binding Kotlin/Swift exatamente
como nos outros samples. O caminho não-trivial é o MCU — e é o que
este sample cobre.

## CI

O target `thumbv8m.main-none-eabihf` é compilado em cada PR via
`ci.yml` (mesma cadeia de `thumbv7em-none-eabihf` e
`riscv32imc-unknown-none-elf`). Quebra de build é detectada antes do
merge.
