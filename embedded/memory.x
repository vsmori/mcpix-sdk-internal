/* Layout de memória genérico para Cortex-M4F (similar a STM32F4 e ESP32-derivados ARM). */
/* Para hardware específico, ajustar os tamanhos abaixo conforme datasheet. */

MEMORY
{
  /* 512 KB flash em ORIGIN 0x08000000 — perfil STM32F407 típico */
  FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 512K
  /* 128 KB SRAM — cobre MCUs que rodam o SDK confortavelmente */
  RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 128K
}
