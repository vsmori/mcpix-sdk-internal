use thiserror::Error;

/// Erros internos do núcleo. Mapeáveis 1:1 para códigos numéricos na FFI
/// — ver `mcpix-ffi/src/error.rs` para a tabela de códigos C-ABI.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum McpixError {
    #[error("transport field has invalid length: expected 26..=35, got {0}")]
    TransportFieldLength(usize),

    #[error("transport field contains non-alphanumeric character at position {0}")]
    TransportFieldCharset(usize),

    #[error("transport field prefix does not match expected scheme")]
    TransportFieldPrefix,

    #[error("seed id has invalid length: expected up to {max}, got {got}")]
    SeedIdLength { max: usize, got: usize },

    #[error("seed id contains non-alphanumeric character")]
    SeedIdCharset,

    #[error("seed material has wrong length: expected {expected}, got {got}")]
    SeedLength { expected: usize, got: usize },

    #[error("counter overflow")]
    CounterOverflow,

    #[error("unknown seed id")]
    UnknownSeed,

    #[error("no retained receipt for the presented charge")]
    NoRetainedReceipt,

    #[error("retained receipt already consumed (replay rejected)")]
    ReplayRejected,

    #[error("confirmation code mismatch")]
    Mismatch,

    #[error("storage failure: {0}")]
    Storage(String),

    #[error("transport failure: {0}")]
    Transport(String),
}
