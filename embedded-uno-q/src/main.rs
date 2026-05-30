//! Demo bare-metal do `mcpix-embed` no MCU do **Arduino UNO Q**
//! (STM32U585, Cortex-M33, ARMv8-M Main).
//!
//! A UNO Q é dual-arquitetura: o STM32U585 cuida do tempo-real / I/O
//! (USB, GPIO, secure storage via TrustZone + AES on-chip), e o
//! Qualcomm Dragonwing roda Debian para UI/conectividade. Este sample
//! cobre o **lado MCU** — o caminho onde o material da Seed e a
//! validação do C₂ nunca tocam um SO de propósito geral.
//!
//! Topologia recomendada em produção:
//!   • Lado MCU (este crate)  → custódia de Seed, derivação `(C₁, C₂)`,
//!                              counter monotônico em flash, validação
//!                              local de C₂ em tempo constante.
//!   • Lado Linux (Dragonwing)→ UI (display, NFC, conectividade), envia
//!                              `(seed_id, amount)` ao MCU via UART/I²C
//!                              e recebe `transport_field` de volta.
//!                              Roda `mcpix-receiver-sdk` se precisar de
//!                              persistência mais rica que a flash do MCU.
//!
//! O fluxo abaixo é idêntico ao demo embedded genérico: persistir T
//! ANTES de derivar, drop dos handles para simular reboot, recarregar
//! e marcar consumed. Ver `embedded/src/main.rs` para a versão Cortex-M4F
//! e o README deste sample para o trade-off MCU vs Linux side.
//!
//! Status (lido por debugger/RTT):
//!   0 = pronto · 1 = derivação OK · 2 = encode OK · 3 = QR OK
//!   4 = persist OK · 5 = reboot+load OK · 6 = consumed marked
//!   0xFF = erro
//!
//! `RamFlash` aqui vive em RAM — o "reboot" é drop+reconstruct no mesmo
//! processo. Em hardware real, substituir por uma impl `NorFlash` do
//! HAL do U585 (e.g. `stm32u5xx-hal::flash::Flash`) ou, melhor ainda,
//! por backend Secure Element via TrustZone para custódia da Seed.

#![no_std]
#![no_main]

use core::hint::black_box;
use core::sync::atomic::{AtomicU8, Ordering};

use cortex_m_rt::entry;
use mcpix_embed::{
    crypto::{derive_pair, verify_c2},
    qr::{charge_qr, QR_BUF_LEN},
    storage::{
        CounterStore, PersistedCounter, PersistedReceipt, RamFlash, ReceiptStore, SLOT_SIZE,
    },
    transport_field::{encode_into, TRANSPORT_FIELD_LEN},
    types::{Seed, SeedId},
};
use panic_halt as _;

static STATUS: AtomicU8 = AtomicU8::new(0);

/// Layout fixo dentro de um RamFlash de 256 bytes:
///   0..64    → ReceiptStore slot A
///   64..128  → ReceiptStore slot B
///   128..192 → CounterStore slot A
///   192..256 → CounterStore slot B
const FLASH_LEN: usize = 256;
const RECEIPT_SLOT_A: u32 = 0;
const RECEIPT_SLOT_B: u32 = SLOT_SIZE as u32;
const COUNTER_SLOT_A: u32 = (SLOT_SIZE * 2) as u32;
const COUNTER_SLOT_B: u32 = (SLOT_SIZE * 3) as u32;

fn halt() -> ! {
    loop {
        cortex_m::asm::wfi();
    }
}

#[entry]
fn main() -> ! {
    // ── Setup ─────────────────────────────────────────────────────────
    // Seed estática para o demo. Em produção no UNO Q, derivar via
    // TrustZone-protected key store do U585 (CKMS + AES on-chip) — ver
    // `docs/SECURE_ELEMENT.md` para o contrato SeedSealer e como expor
    // o material só dentro do mundo seguro.
    let seed = Seed::from_bytes([0xAB; 32]);
    let seed_id = match SeedId::new("UQ1") {
        Ok(s) => s,
        Err(_) => {
            STATUS.store(0xFF, Ordering::SeqCst);
            halt();
        }
    };

    // RamFlash simula a flash do MCU. Em produção, substituir por
    // `stm32u5xx_hal::flash::Flash` (ou crate equivalente do U585).
    let mut flash = RamFlash::<FLASH_LEN>::new();

    // ── Recuperação do counter persistido ─────────────────────────────
    let last_t = {
        let mut store = CounterStore::new(flash, COUNTER_SLOT_A, COUNTER_SLOT_B);
        let t = store.load().map(|c| c.last_t).unwrap_or(0);
        flash = store.free();
        t
    };
    let new_t = last_t + 1;

    // Persiste o novo T ANTES de gerar a cobrança — assim, queda de
    // energia no meio nunca permite reuso do T. Crítico em IoT: o UNO Q
    // pode ser desconectado a qualquer momento pelo usuário final.
    {
        let mut store = CounterStore::new(flash, COUNTER_SLOT_A, COUNTER_SLOT_B);
        if store
            .save(&PersistedCounter {
                seed_id: seed_id.clone(),
                last_t: new_t,
            })
            .is_err()
        {
            STATUS.store(0xFF, Ordering::SeqCst);
            halt();
        }
        flash = store.free();
    }

    // ── Derivação ────────────────────────────────────────────────────
    let (c1, c2_retained) = derive_pair(&seed, new_t);
    STATUS.store(1, Ordering::SeqCst);

    // ── Encode do campo de transporte ────────────────────────────────
    let mut transport_buf = [0u8; TRANSPORT_FIELD_LEN];
    let field = encode_into(&seed_id, &c1, &mut transport_buf);
    let _ = black_box(field.len());
    STATUS.store(2, Ordering::SeqCst);

    // ── QR encoding ──────────────────────────────────────────────────
    // O QR roda dentro do MCU em ~5ms — ideal para piscar num display
    // OLED via SPI no lado Dragonwing antes de o pagador apresentar
    // o C₂ via NFC/câmera.
    let mut tmp = [0u8; QR_BUF_LEN];
    let mut out = [0u8; QR_BUF_LEN];
    if let Ok(qr) = charge_qr(field, &mut tmp, &mut out) {
        let _ = black_box(qr.size());
        STATUS.store(3, Ordering::SeqCst);
    } else {
        STATUS.store(0xFF, Ordering::SeqCst);
        halt();
    }

    // ── Persiste C₂ retido em flash ──────────────────────────────────
    let receipt_to_save = PersistedReceipt {
        seed_id: seed_id.clone(),
        counter: new_t,
        amount_cents: 9900,
        expected_c2: c2_retained.clone(),
        consumed: false,
    };
    {
        let mut store = ReceiptStore::new(flash, RECEIPT_SLOT_A, RECEIPT_SLOT_B);
        if store.save(&receipt_to_save).is_err() {
            STATUS.store(0xFF, Ordering::SeqCst);
            halt();
        }
        flash = store.free();
    }
    STATUS.store(4, Ordering::SeqCst);

    // ── Simula reboot ────────────────────────────────────────────────
    // `c2_retained` carrega material sensível e implementa
    // `ZeroizeOnDrop` — dropamos explicitamente antes do "reload" para
    // garantir zeroização (em hardware real, o cold-boot já apagaria
    // a SRAM, mas o drop garante isso em qualquer cenário).
    drop(c2_retained);

    // Re-abre ambos os stores e lê o que ficou persistido.
    let loaded_receipt = {
        let mut store = ReceiptStore::new(flash, RECEIPT_SLOT_A, RECEIPT_SLOT_B);
        let r = match store.load() {
            Ok(r) => r,
            Err(_) => {
                STATUS.store(0xFF, Ordering::SeqCst);
                halt();
            }
        };
        flash = store.free();
        r
    };

    let loaded_counter = {
        let mut store = CounterStore::new(flash, COUNTER_SLOT_A, COUNTER_SLOT_B);
        let c = match store.load() {
            Ok(c) => c,
            Err(_) => {
                STATUS.store(0xFF, Ordering::SeqCst);
                halt();
            }
        };
        flash = store.free();
        c
    };

    if loaded_receipt != receipt_to_save || loaded_counter.last_t != new_t {
        STATUS.store(0xFF, Ordering::SeqCst);
        halt();
    }
    STATUS.store(5, Ordering::SeqCst);

    // ── Validação local em tempo constante ───────────────────────────
    // O C₂ apresentado pelo pagador chegaria via NFC (lado Dragonwing
    // lê e encaminha por UART) ou via câmera/OCR. Aqui simulamos com o
    // próprio C₂ esperado para fechar o ciclo end-to-end.
    if verify_c2(&loaded_receipt.expected_c2, &loaded_receipt.expected_c2) {
        // Marca consumed em flash — defesa de replay sobrevive a reboot.
        let mut store = ReceiptStore::new(flash, RECEIPT_SLOT_A, RECEIPT_SLOT_B);
        if store.mark_consumed().is_err() {
            STATUS.store(0xFF, Ordering::SeqCst);
            halt();
        }
        STATUS.store(6, Ordering::SeqCst);
    } else {
        STATUS.store(0xFF, Ordering::SeqCst);
    }

    halt();
}
