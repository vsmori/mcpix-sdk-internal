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

    #[error("transport field carries unsupported protocol version: '{0}' — atualize a SDK")]
    UnsupportedProtocolVersion(String),

    #[error("seed id has invalid length: expected up to {max}, got {got}")]
    SeedIdLength { max: usize, got: usize },

    #[error("seed id contains non-alphanumeric character")]
    SeedIdCharset,

    #[error("seed material has wrong length: expected {expected}, got {got}")]
    SeedLength { expected: usize, got: usize },

    #[error("counter overflow")]
    CounterOverflow,

    #[error("counter collision: another charge already issued in the current window ({window_seconds}s)")]
    CounterCollision { window_seconds: u64 },

    #[error("counter rollback detected: clock moved backwards (last={last}, now={now})")]
    CounterRollback { last: u64, now: u64 },

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
