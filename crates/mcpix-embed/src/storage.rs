//! Persistência embarcada de C₂ retido e do contador `T`.
//!
//! Por que existe — gap reconhecido em THREAT_MODEL §7.2: no S9, o
//! recebedor embarcado retém `C₂` em RAM. Se o microcontrolador reinicia
//! (queda de energia, watchdog reset) antes de o comprovante chegar, o
//! retained é perdido — comprovante futuro não tem como ser validado.
//! Análogo ao contador `T` sequencial.
//!
//! Solução: persistir em flash não-volátil usando a abstração padrão
//! `embedded-storage::NorFlash`. Caller provê o backend específico:
//!
//! - **ESP32 family**: [`esp-storage`](https://crates.io/crates/esp-storage)
//! - **STM32**: `stm32xx-hal::flash` (varia por sub-família)
//! - **nRF52/53**: `nrf-softdevice::flash` ou `nrf-hal::nvmc`
//! - **Testes**: `RamFlash` provido aqui, simula um NorFlash em RAM.
//!
//! ## Estratégia de durabilidade
//!
//! **Ping-pong de 2 slots por record type.** Cada save escreve no slot
//! "mais antigo" (após erase de sector) com `save_seq` incrementado.
//! Load lê ambos os slots, valida CRC32, e retorna o slot com maior
//! `save_seq` válido.
//!
//! Garantias:
//!
//! - **Atomicidade de cold-cut**: queda de energia durante o save
//!   compromete *apenas* o slot novo; o anterior continua íntegro e
//!   é o que `load` retorna.
//! - **Detecção de corrupção**: CRC32 sobre todo o record (exceto
//!   o próprio CRC). Slot com CRC inválido é ignorado em load.
//! - **Wear-leveling básico**: alterna entre dois sectors. Para um
//!   device com 100k ciclos por sector, dobra para 200k saves antes
//!   da reescrita do sector.
//!
//! ## Layout
//!
//! Cada slot ocupa exatamente `SLOT_SIZE = 64` bytes (cabe em qualquer
//! sector real ≥ 256 bytes). Layout do record:
//!
//! ```text
//!   ┌───────┬─────────┬──────────┬───────────────┬─────────┬──────────────┬──────────┬──────┬───────┬─────────┐
//!   │ magic │ version │ save_seq │ seed_id_len 1 │ seed_id │ counter (T)  │ amount   │ C₂   │ flags │ crc32   │
//!   │ 4     │ 2       │ 4        │ 1             │ 16      │ 8            │ 8        │ 11   │ 1     │ 4       │
//!   └───────┴─────────┴──────────┴───────────────┴─────────┴──────────────┴──────────┴──────┴───────┴─────────┘
//!     0       4         6          10              11        27             35         43     54      55..59
//!   (record body = 55 bytes; resto do slot é padding com 0xFF — estado
//!   pós-erase de NOR flash)
//! ```
//!
//! `flags` bit 0 = `consumed`. Reservados para uso futuro: bits 1..7.

use core::convert::TryInto;

use crc::{Crc, CRC_32_ISO_HDLC};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};

use crate::types::{EmbedError, SeedId, C2, C2_LEN, SEED_ID_MAX_LEN};

const MAGIC_RETAINED: [u8; 4] = *b"MR01"; // Mcpix Retained v01
const MAGIC_COUNTER: [u8; 4] = *b"MC01"; // Mcpix Counter v01
const VERSION: u16 = 1;
pub const SLOT_SIZE: usize = 64;

/// CRC-32/ISO-HDLC — mesmo polinômio do `crc32` clássico (gzip, zip, etc.).
const CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

#[derive(Debug, PartialEq, Eq)]
pub enum StorageError {
    /// Erro do backend de flash (read/write/erase). Caller pode inspecionar
    /// pelo logger do backend; aqui só sinalizamos.
    Backend,
    /// Nenhum slot válido — store nunca foi inicializado, ou ambos
    /// sectors corrompidos.
    Empty,
    /// Record desserializado com SeedId mal-formado (cert. corrupto que
    /// passou no CRC mas semanticamente inválido — improvável).
    Malformed,
    /// Buffer fornecido pelo caller é menor que o esperado.
    InsufficientBuffer,
}

/// Record desserializado do C₂ retido. Espelho fiel do que estava em
/// flash, validado por CRC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedReceipt {
    pub seed_id: SeedId,
    pub counter: u64,
    pub amount_cents: u64,
    pub expected_c2: C2,
    pub consumed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedCounter {
    pub seed_id: SeedId,
    pub last_t: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// (De)serialização do record do C₂ retido
// ─────────────────────────────────────────────────────────────────────────────

fn write_retained_record(buf: &mut [u8; SLOT_SIZE], save_seq: u32, receipt: &PersistedReceipt) {
    // Pre-fill com 0xFF (estado pós-erase de NOR; idempotente).
    buf.fill(0xFF);

    buf[0..4].copy_from_slice(&MAGIC_RETAINED);
    buf[4..6].copy_from_slice(&VERSION.to_be_bytes());
    buf[6..10].copy_from_slice(&save_seq.to_be_bytes());

    let sid = receipt.seed_id.as_bytes();
    buf[10] = sid.len() as u8;
    // Slot fixo de 16 bytes; restante zerado (sentinela neutro, não 0xFF
    // para evitar conflito com erased state em comparações).
    let sid_start = 11;
    let sid_end = 11 + SEED_ID_MAX_LEN;
    buf[sid_start..sid_start + sid.len()].copy_from_slice(sid);
    for byte in &mut buf[sid_start + sid.len()..sid_end] {
        *byte = 0;
    }

    buf[27..35].copy_from_slice(&receipt.counter.to_be_bytes());
    buf[35..43].copy_from_slice(&receipt.amount_cents.to_be_bytes());
    buf[43..54].copy_from_slice(receipt.expected_c2.as_bytes());
    buf[54] = if receipt.consumed { 1 } else { 0 };

    let crc = CRC32.checksum(&buf[..55]);
    buf[55..59].copy_from_slice(&crc.to_be_bytes());
}

fn read_retained_record(buf: &[u8; SLOT_SIZE]) -> Option<(u32, PersistedReceipt)> {
    if &buf[0..4] != MAGIC_RETAINED.as_slice() {
        return None;
    }
    let version = u16::from_be_bytes(buf[4..6].try_into().ok()?);
    if version != VERSION {
        return None;
    }
    let crc_expected = u32::from_be_bytes(buf[55..59].try_into().ok()?);
    let crc_actual = CRC32.checksum(&buf[..55]);
    if crc_actual != crc_expected {
        return None;
    }
    let save_seq = u32::from_be_bytes(buf[6..10].try_into().ok()?);
    let sid_len = buf[10] as usize;
    if sid_len == 0 || sid_len > SEED_ID_MAX_LEN {
        return None;
    }
    let sid_str = core::str::from_utf8(&buf[11..11 + sid_len]).ok()?;
    let seed_id = SeedId::new(sid_str).ok()?;

    let counter = u64::from_be_bytes(buf[27..35].try_into().ok()?);
    let amount_cents = u64::from_be_bytes(buf[35..43].try_into().ok()?);
    let mut c2_bytes = [0u8; C2_LEN];
    c2_bytes.copy_from_slice(&buf[43..54]);
    let expected_c2 = C2(c2_bytes);
    let consumed = buf[54] != 0;

    Some((
        save_seq,
        PersistedReceipt {
            seed_id,
            counter,
            amount_cents,
            expected_c2,
            consumed,
        },
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// (De)serialização do record do counter
// ─────────────────────────────────────────────────────────────────────────────

fn write_counter_record(buf: &mut [u8; SLOT_SIZE], save_seq: u32, counter: &PersistedCounter) {
    buf.fill(0xFF);
    buf[0..4].copy_from_slice(&MAGIC_COUNTER);
    buf[4..6].copy_from_slice(&VERSION.to_be_bytes());
    buf[6..10].copy_from_slice(&save_seq.to_be_bytes());

    let sid = counter.seed_id.as_bytes();
    buf[10] = sid.len() as u8;
    let sid_start = 11;
    let sid_end = 11 + SEED_ID_MAX_LEN;
    buf[sid_start..sid_start + sid.len()].copy_from_slice(sid);
    for byte in &mut buf[sid_start + sid.len()..sid_end] {
        *byte = 0;
    }
    buf[27..35].copy_from_slice(&counter.last_t.to_be_bytes());
    let crc = CRC32.checksum(&buf[..35]);
    buf[35..39].copy_from_slice(&crc.to_be_bytes());
}

fn read_counter_record(buf: &[u8; SLOT_SIZE]) -> Option<(u32, PersistedCounter)> {
    if &buf[0..4] != MAGIC_COUNTER.as_slice() {
        return None;
    }
    let version = u16::from_be_bytes(buf[4..6].try_into().ok()?);
    if version != VERSION {
        return None;
    }
    let crc_expected = u32::from_be_bytes(buf[35..39].try_into().ok()?);
    let crc_actual = CRC32.checksum(&buf[..35]);
    if crc_actual != crc_expected {
        return None;
    }
    let save_seq = u32::from_be_bytes(buf[6..10].try_into().ok()?);
    let sid_len = buf[10] as usize;
    if sid_len == 0 || sid_len > SEED_ID_MAX_LEN {
        return None;
    }
    let sid_str = core::str::from_utf8(&buf[11..11 + sid_len]).ok()?;
    let seed_id = SeedId::new(sid_str).ok()?;
    let last_t = u64::from_be_bytes(buf[27..35].try_into().ok()?);
    Some((save_seq, PersistedCounter { seed_id, last_t }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Store ping-pong
// ─────────────────────────────────────────────────────────────────────────────

/// Store de C₂ retido em dois slots de flash. Caller informa offsets
/// **sector-aligned** dos dois slots na construção.
///
/// Pré-requisito: `slot_a_offset` e `slot_b_offset` apontam para sectors
/// distintos cuja erase-size cobre `SLOT_SIZE` (64 bytes). NOR flash
/// típica tem sectors de 4 KB; caller usa apenas 64 bytes de cada um.
pub struct ReceiptStore<F: NorFlash> {
    flash: F,
    slot_a: u32,
    slot_b: u32,
}

impl<F: NorFlash> ReceiptStore<F> {
    pub fn new(flash: F, slot_a: u32, slot_b: u32) -> Self {
        Self {
            flash,
            slot_a,
            slot_b,
        }
    }

    pub fn free(self) -> F {
        self.flash
    }

    /// Persiste o receipt no slot mais antigo (ou no `slot_a` se ambos
    /// vazios). Retorna o `save_seq` usado.
    pub fn save(&mut self, receipt: &PersistedReceipt) -> Result<u32, StorageError> {
        let (target_offset, next_seq) = self.choose_save_target_retained()?;
        let mut buf = [0u8; SLOT_SIZE];
        write_retained_record(&mut buf, next_seq, receipt);
        self.flash
            .erase(target_offset, target_offset + SLOT_SIZE as u32)
            .map_err(|_| StorageError::Backend)?;
        self.flash
            .write(target_offset, &buf)
            .map_err(|_| StorageError::Backend)?;
        Ok(next_seq)
    }

    /// Lê o slot mais recente válido. Retorna `Empty` se nenhum slot
    /// passou na validação de CRC.
    pub fn load(&mut self) -> Result<PersistedReceipt, StorageError> {
        let a = self.read_slot_retained(self.slot_a)?;
        let b = self.read_slot_retained(self.slot_b)?;
        match (a, b) {
            (None, None) => Err(StorageError::Empty),
            (Some((_, r)), None) | (None, Some((_, r))) => Ok(r),
            (Some((sa, ra)), Some((sb, rb))) => {
                if sa >= sb {
                    Ok(ra)
                } else {
                    Ok(rb)
                }
            }
        }
    }

    /// Marca `consumed = true` salvando uma cópia atualizada no outro
    /// slot. Idempotente: chamar duas vezes resulta em consumed=true.
    pub fn mark_consumed(&mut self) -> Result<(), StorageError> {
        let mut current = self.load()?;
        if current.consumed {
            return Ok(());
        }
        current.consumed = true;
        self.save(&current)?;
        Ok(())
    }

    fn read_slot_retained(
        &mut self,
        offset: u32,
    ) -> Result<Option<(u32, PersistedReceipt)>, StorageError> {
        let mut buf = [0u8; SLOT_SIZE];
        self.flash
            .read(offset, &mut buf)
            .map_err(|_| StorageError::Backend)?;
        Ok(read_retained_record(&buf))
    }

    fn choose_save_target_retained(&mut self) -> Result<(u32, u32), StorageError> {
        let a = self.read_slot_retained(self.slot_a)?;
        let b = self.read_slot_retained(self.slot_b)?;
        // Estratégia: escrever no slot "mais antigo" para que, em queda
        // de energia durante o write, o slot oposto (mais novo válido)
        // continue íntegro.
        let next = match (&a, &b) {
            (None, None) => 1,
            (Some((s, _)), None) => s + 1,
            (None, Some((s, _))) => s + 1,
            (Some((sa, _)), Some((sb, _))) => sa.max(sb) + 1,
        };
        let target = match (&a, &b) {
            (None, _) => self.slot_a,
            (_, None) => self.slot_b,
            (Some((sa, _)), Some((sb, _))) => {
                if sa <= sb {
                    self.slot_a
                } else {
                    self.slot_b
                }
            }
        };
        Ok((target, next))
    }
}

/// Store do contador `T` — mesma estratégia, magic distinta.
pub struct CounterStore<F: NorFlash> {
    flash: F,
    slot_a: u32,
    slot_b: u32,
}

impl<F: NorFlash> CounterStore<F> {
    pub fn new(flash: F, slot_a: u32, slot_b: u32) -> Self {
        Self {
            flash,
            slot_a,
            slot_b,
        }
    }

    pub fn free(self) -> F {
        self.flash
    }

    pub fn save(&mut self, counter: &PersistedCounter) -> Result<u32, StorageError> {
        let (target_offset, next_seq) = self.choose_save_target_counter()?;
        let mut buf = [0u8; SLOT_SIZE];
        write_counter_record(&mut buf, next_seq, counter);
        self.flash
            .erase(target_offset, target_offset + SLOT_SIZE as u32)
            .map_err(|_| StorageError::Backend)?;
        self.flash
            .write(target_offset, &buf)
            .map_err(|_| StorageError::Backend)?;
        Ok(next_seq)
    }

    pub fn load(&mut self) -> Result<PersistedCounter, StorageError> {
        let a = self.read_slot_counter(self.slot_a)?;
        let b = self.read_slot_counter(self.slot_b)?;
        match (a, b) {
            (None, None) => Err(StorageError::Empty),
            (Some((_, c)), None) | (None, Some((_, c))) => Ok(c),
            (Some((sa, ca)), Some((sb, cb))) => {
                if sa >= sb {
                    Ok(ca)
                } else {
                    Ok(cb)
                }
            }
        }
    }

    fn read_slot_counter(
        &mut self,
        offset: u32,
    ) -> Result<Option<(u32, PersistedCounter)>, StorageError> {
        let mut buf = [0u8; SLOT_SIZE];
        self.flash
            .read(offset, &mut buf)
            .map_err(|_| StorageError::Backend)?;
        Ok(read_counter_record(&buf))
    }

    fn choose_save_target_counter(&mut self) -> Result<(u32, u32), StorageError> {
        let a = self.read_slot_counter(self.slot_a)?;
        let b = self.read_slot_counter(self.slot_b)?;
        let next = match (&a, &b) {
            (None, None) => 1,
            (Some((s, _)), None) => s + 1,
            (None, Some((s, _))) => s + 1,
            (Some((sa, _)), Some((sb, _))) => sa.max(sb) + 1,
        };
        let target = match (&a, &b) {
            (None, _) => self.slot_a,
            (_, None) => self.slot_b,
            (Some((sa, _)), Some((sb, _))) => {
                if sa <= sb {
                    self.slot_a
                } else {
                    self.slot_b
                }
            }
        };
        Ok((target, next))
    }
}

// Silencia warning de import quando feature `qr` está off (mantemos
// EmbedError disponível como parte da API pública do crate).
#[allow(dead_code)]
fn _force_use(_: EmbedError) {}

// ─────────────────────────────────────────────────────────────────────────────
// RamFlash — backend in-memory para testes e demo bare-metal de "reboot"
// ─────────────────────────────────────────────────────────────────────────────

/// `NorFlash` simulada em RAM. Sector e write granularities configuráveis
/// via const generics para casar com NOR flash reais (typical 4096/4 ou
/// 4096/256). Default ergonômico para testes.
pub struct RamFlash<const N: usize> {
    pub bytes: [u8; N],
}

impl<const N: usize> RamFlash<N> {
    pub const fn new() -> Self {
        // Estado pós-erase de NOR é 0xFF.
        Self { bytes: [0xFF; N] }
    }
}

impl<const N: usize> Default for RamFlash<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct RamFlashError;

impl<const N: usize> embedded_storage::nor_flash::ErrorType for RamFlash<N> {
    type Error = RamFlashError;
}

impl embedded_storage::nor_flash::NorFlashError for RamFlashError {
    fn kind(&self) -> embedded_storage::nor_flash::NorFlashErrorKind {
        embedded_storage::nor_flash::NorFlashErrorKind::Other
    }
}

impl<const N: usize> ReadNorFlash for RamFlash<N> {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let start = offset as usize;
        let end = start + bytes.len();
        if end > N {
            return Err(RamFlashError);
        }
        bytes.copy_from_slice(&self.bytes[start..end]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        N
    }
}

impl<const N: usize> NorFlash for RamFlash<N> {
    // Granularidades pequenas para os testes; ainda alinha com SLOT_SIZE.
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = 64;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let start = from as usize;
        let end = to as usize;
        if end > N || start > end {
            return Err(RamFlashError);
        }
        for byte in &mut self.bytes[start..end] {
            *byte = 0xFF;
        }
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let start = offset as usize;
        let end = start + bytes.len();
        if end > N {
            return Err(RamFlashError);
        }
        // NOR flash real só permite bit-clearing (0→1 não, 1→0 sim).
        // Simulamos para pegar bugs onde caller esqueceu erase antes:
        for (slot, &new) in self.bytes[start..end].iter_mut().zip(bytes) {
            *slot &= new;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::derive_pair;
    use crate::types::Seed;

    fn fresh_receipt() -> PersistedReceipt {
        let seed = Seed::from_bytes([0x42; 32]);
        let (_, c2) = derive_pair(&seed, 1);
        PersistedReceipt {
            seed_id: SeedId::new("R1").unwrap(),
            counter: 1,
            amount_cents: 9900,
            expected_c2: c2,
            consumed: false,
        }
    }

    #[test]
    fn save_then_load_roundtrip() {
        let mut store = ReceiptStore::new(RamFlash::<256>::new(), 0, 64);
        let r = fresh_receipt();
        store.save(&r).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded, r);
    }

    #[test]
    fn empty_store_returns_empty() {
        let mut store = ReceiptStore::new(RamFlash::<256>::new(), 0, 64);
        assert_eq!(store.load(), Err(StorageError::Empty));
    }

    #[test]
    fn second_save_lands_on_other_slot() {
        let mut store = ReceiptStore::new(RamFlash::<256>::new(), 0, 64);
        let mut r = fresh_receipt();
        let seq1 = store.save(&r).unwrap();
        r.amount_cents = 12345;
        let seq2 = store.save(&r).unwrap();
        assert!(seq2 > seq1);
        let loaded = store.load().unwrap();
        assert_eq!(loaded.amount_cents, 12345);
    }

    #[test]
    fn mark_consumed_is_persistent() {
        let mut store = ReceiptStore::new(RamFlash::<256>::new(), 0, 64);
        store.save(&fresh_receipt()).unwrap();
        store.mark_consumed().unwrap();

        // Simula reboot: drop store, reabre sobre o mesmo flash.
        let flash = store.free();
        let mut reloaded = ReceiptStore::new(flash, 0, 64);
        assert!(reloaded.load().unwrap().consumed);
    }

    #[test]
    fn corruption_in_one_slot_doesnt_kill_load() {
        let mut store = ReceiptStore::new(RamFlash::<256>::new(), 0, 64);
        store.save(&fresh_receipt()).unwrap();
        // Corrompe slot A (onde o primeiro save caiu).
        store.flash.bytes[10] ^= 0x55;
        // Re-save indo para slot B com seq mais alta.
        let mut r = fresh_receipt();
        r.amount_cents = 7777;
        store.save(&r).unwrap();
        // Slot A corrompido (CRC inválido), slot B válido — load deve
        // retornar slot B.
        let loaded = store.load().unwrap();
        assert_eq!(loaded.amount_cents, 7777);
    }

    #[test]
    fn counter_save_load_roundtrip() {
        let mut store = CounterStore::new(RamFlash::<256>::new(), 128, 192);
        let c = PersistedCounter {
            seed_id: SeedId::new("R1").unwrap(),
            last_t: 56_666_667,
        };
        store.save(&c).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded, c);
    }

    #[test]
    fn counter_survives_simulated_reboot() {
        let mut store = CounterStore::new(RamFlash::<256>::new(), 128, 192);
        store
            .save(&PersistedCounter {
                seed_id: SeedId::new("R1").unwrap(),
                last_t: 42,
            })
            .unwrap();
        // "Reboot": drop, re-construct with same flash bytes.
        let flash = store.free();
        let mut reloaded = CounterStore::new(flash, 128, 192);
        assert_eq!(reloaded.load().unwrap().last_t, 42);
    }

    #[test]
    fn both_stores_coexist_in_same_flash() {
        // Verifica que magic distinta evita confusão entre records.
        let mut flash = RamFlash::<256>::new();

        // Setup: usa o mesmo backend para ambos.
        // Construir os stores um de cada vez (consome flash; reconstruímos
        // do free).
        let mut rstore = ReceiptStore::new(flash, 0, 64);
        rstore.save(&fresh_receipt()).unwrap();
        flash = rstore.free();

        let mut cstore = CounterStore::new(flash, 128, 192);
        cstore
            .save(&PersistedCounter {
                seed_id: SeedId::new("R1").unwrap(),
                last_t: 99,
            })
            .unwrap();
        flash = cstore.free();

        // Load de cada lado deve retornar seu próprio record.
        let mut rstore = ReceiptStore::new(flash, 0, 64);
        assert_eq!(rstore.load().unwrap().counter, 1);
        let flash = rstore.free();

        let mut cstore = CounterStore::new(flash, 128, 192);
        assert_eq!(cstore.load().unwrap().last_t, 99);
    }
}
