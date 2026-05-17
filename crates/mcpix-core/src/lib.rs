//! mcpix-core — núcleo criptográfico e de protocolo, isolado de I/O.
//!
//! Princípios (ver ESPECIFICAÇÃO TÉCNICA, Bloco 1):
//! - Sem I/O direto: rede, disco e hardware seguro entram via traits injetadas.
//! - Estado imutável: transições são funcionais — `f(Estado, Comando) -> NovoEstado`.
//! - Sem `panic` em caminho público: erros retornam `Result<_, McpixError>`.
//! - Comparações criptográficas em tempo constante.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]

pub mod crypto;
pub mod error;
pub mod integrity;
pub mod state;
pub mod traits;
pub mod transport_field;
pub mod types;

pub use error::McpixError;
pub use types::{C1, C2, Charge, ConfirmationCode, RetainedReceipt, Seed, SeedId};
