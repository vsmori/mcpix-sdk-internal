/* STM32U585 (Arduino UNO Q, MCU side) — Cortex-M33, TrustZone-capable.
 * Datasheet: 2 MB Flash (não-banked), 786 KB SRAM total.
 * Layout abaixo usa SRAM1+SRAM2 contíguos (192 KB + 64 KB = 256 KB)
 * — sobra suficiente sem precisar mapear SRAM3/SRAM4 que ficam em
 * endereços não-contíguos. Para uso de TrustZone (NSC/SAU) ajustar
 * via .cargo/config.toml + cortex-m-rt features.
 */

MEMORY
{
  FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 2048K
  RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 256K
}
