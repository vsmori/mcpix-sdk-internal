//! Demo bare-metal do `mcpix-embed` — agora com **persistência em flash**.
//!
//! Fluxo demonstrado:
//!   1. Cria `Seed` estática (em produção viria de Secure Element/eFuse)
//!   2. Lê `last_t` do `CounterStore` (flash); 0 se primeiro boot
//!   3. Avança T; persiste em flash antes de derivar (anti-rollback de
//!      energia perdida no meio)
//!   4. Deriva `(C₁, C₂)`, encoda no campo de transporte, gera QR
//!   5. Persiste `C₂` retido no `ReceiptStore`
//!   6. **Simula reboot**: drop dos stores, reabre sobre o mesmo flash
//!   7. Verifica que `load()` recupera o retained e o counter idênticos
//!   8. Marca `consumed` (cobranças posteriores não revalidam)
//!
//! Status (lido por debugger):
//!   0 = pronto · 1 = derivação OK · 2 = encode OK · 3 = QR OK
//!   4 = persist OK · 5 = reboot+load OK · 6 = consumed marked
//!   0xFF = erro
//!
//! Como `RamFlash` vive em RAM aqui, o "reboot" é simulação dentro do
//! mesmo processo (drop+reconstruct). Em hardware real, o `NorFlash`
//! seria `esp_storage::FlashStorage`, `stm32xx_hal::flash::FLASH`, etc.,
//! e o reboot físico preserva os bytes.

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
    let seed = Seed::from_bytes([0xAB; 32]);
    let seed_id = match SeedId::new("R1") {
        Ok(s) => s,
        Err(_) => {
            STATUS.store(0xFF, Ordering::SeqCst);
            halt();
        }
    };

    // RamFlash simula a flash do MCU. Em produção, substituir por
    // `esp_storage::FlashStorage` ou equivalente.
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
    // energia no meio nunca permite reuso do T.
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
    // (drop tudo que vive em RAM, restando apenas `flash` bytes — análogo
    // ao que sobrevive a cold-boot do MCU. Em hardware real, seria
    // simplesmente o conteúdo da NOR pós-power-cycle.)
    // Apenas `c2_retained` carrega material sensível e implementa
    // `ZeroizeOnDrop` — explicitamente dropamos para garantir zeroização
    // antes do reload. `c1` e `transport_buf` são `Copy`/inertes.
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

    // Sanity: o que carregamos bate com o que salvamos.
    if loaded_receipt != receipt_to_save || loaded_counter.last_t != new_t {
        STATUS.store(0xFF, Ordering::SeqCst);
        halt();
    }
    STATUS.store(5, Ordering::SeqCst);

    // ── Validação local em tempo constante ───────────────────────────
    // Em uso real, o C₂ apresentado viria via OCR/digitação. Aqui
    // simulamos com o próprio C₂ esperado para fechar o ciclo end-to-end.
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
