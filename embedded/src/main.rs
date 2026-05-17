//! Demo bare-metal do `mcpix-embed`.
//!
//! Fluxo (sem hardware periférico — apenas exercita as APIs):
//!   1. Cria `Seed` estática (em produção viria de Secure Element/eFuse)
//!   2. Deriva `(C₁, C₂)` para counter = 1
//!   3. Encoda no campo de transporte de 35 chars
//!   4. Gera matriz QR pronta para envio a display SPI/I²C
//!   5. Loop infinito
//!
//! Status do último ciclo escrito em `STATUS` (lido por debugger):
//!   0 = pronto, 1 = derivação OK, 2 = encode OK, 3 = QR OK, 0xFF = erro.
//!
//! Para portar a ESP8266 Xtensa: substituir `cortex-m-rt` por
//! `esp8266-hal` + `xtensa-lx-rt`. As chamadas a `derive_pair`,
//! `encode_into` e `charge_qr` **não mudam**.

#![no_std]
#![no_main]

use core::hint::black_box;
use core::sync::atomic::{AtomicU8, Ordering};

use cortex_m_rt::entry;
use mcpix_embed::{
    crypto::derive_pair,
    qr::{charge_qr, QR_BUF_LEN},
    transport_field::{encode_into, TRANSPORT_FIELD_LEN},
    types::{Seed, SeedId},
};
use panic_halt as _;

/// Status do último ciclo de operação, observável via debugger.
static STATUS: AtomicU8 = AtomicU8::new(0);

#[entry]
fn main() -> ! {
    // ── Setup do recebedor ────────────────────────────────────────────
    // Em produção: ler material da eFuse / Secure Element.
    let seed = Seed::from_bytes([0xAB; 32]);
    let seed_id = match SeedId::new("R1") {
        Ok(s) => s,
        Err(_) => {
            STATUS.store(0xFF, Ordering::SeqCst);
            loop {
                cortex_m::asm::wfi();
            }
        }
    };

    // ── Derivação do par (C₁, C₂) ────────────────────────────────────
    let (c1, _c2_retained) = derive_pair(&seed, 1);
    STATUS.store(1, Ordering::SeqCst);
    // `c2_retained` ficaria em SeedStore local (flash, EEPROM) — aqui
    // descartamos para a demo.

    // ── Encode do campo de transporte (35 chars) ─────────────────────
    let mut transport_buf = [0u8; TRANSPORT_FIELD_LEN];
    let field = encode_into(&seed_id, &c1, &mut transport_buf);
    let _field_len = black_box(field.len());
    STATUS.store(2, Ordering::SeqCst);

    // ── QR encoding em buffer fixo ────────────────────────────────────
    let mut tmp = [0u8; QR_BUF_LEN];
    let mut out = [0u8; QR_BUF_LEN];
    match charge_qr(field, &mut tmp, &mut out) {
        Ok(qr) => {
            // Em produção: SPI/I²C driver lê `qr.get_module(x, y)` para cada
            // pixel e empurra para o display. Aqui só forçamos a otimização
            // a não descartar o trabalho.
            let _size = black_box(qr.size());
            for y in 0..qr.size() {
                for x in 0..qr.size() {
                    let _ = black_box(qr.get_module(x, y));
                }
            }
            STATUS.store(3, Ordering::SeqCst);
        }
        Err(_) => STATUS.store(0xFF, Ordering::SeqCst),
    }

    loop {
        cortex_m::asm::wfi();
    }
}
