//! mcpix-embed — núcleo receiver-only para microcontroladores.
//!
//! Subset deliberadamente mínimo do `mcpix-core` que cabe em ESP8266/ESP32/Cortex-M:
//!
//! - **Sem `std`, sem `alloc`**: todas as estruturas vivem na stack ou em
//!   buffers estáticos. Footprint previsível, sem fragmentação de heap.
//! - **Sem rede, sem persistência**: o dispositivo embarcado mostra um QR;
//!   a propagação de C₂ e a validação fim-a-fim ficam em outro componente
//!   (gateway companion, app pareado).
//! - **Algoritmo idêntico**: HMAC-SHA-256 + alfabeto base32 custom +
//!   domain separation `mcpix/v1/c1` e `mcpix/v1/c2`. Cross-validado contra
//!   `mcpix-core` em `tests/cross_validate.rs`.
//!
//! ## API mínima
//!
//! ```text
//!   derive_pair(&seed, counter) -> (C1, C2)
//!   encode_transport_field(&seed_id, &c1) -> heapless::String<35>
//!   #[cfg(feature = "qr")] charge_qr(...) -> Matrix<...>
//! ```
//!
//! ## Threat model embarcado
//!
//! - Side-channel timing em `verify_c2` continua via `subtle::ConstantTimeEq`.
//! - Material da semente é zeroizado quando `Seed` cai de escopo.
//! - **Não cobrimos** ataques físicos (glitch, side-channel power) — para
//!   isso o material deve ficar em Secure Element externo (ATECC608A,
//!   OPTIGA, etc.) e o SDK consome via interface I²C/SPI.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]

pub mod crypto;
pub mod transport_field;
pub mod types;

#[cfg(feature = "qr")]
pub mod qr;

#[cfg(feature = "storage")]
pub mod storage;

pub use crypto::{derive_c2_from_c1, derive_pair, verify_c2};
pub use transport_field::{encode_into, parse_into, PROTOCOL_PREFIX, TRANSPORT_FIELD_LEN};
pub use types::{C1, C2, Seed, SeedId, C1_LEN, C2_LEN, SEED_ID_MAX_LEN, SEED_LEN};
