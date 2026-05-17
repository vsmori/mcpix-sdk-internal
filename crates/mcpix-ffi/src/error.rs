//! Códigos numéricos C-ABI para os erros do núcleo.
//!
//! Mantém ABI estável: nunca renumerar valores existentes; novos erros
//! recebem códigos novos. Os bindings nativos espelham este enum (Swift
//! `enum`, Kotlin `enum class`, C# `enum`).

use mcpix_core::error::McpixError;

#[repr(i32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum McpixStatus {
    Ok = 0,
    InvalidArgument = 1,
    TransportFieldLength = 2,
    TransportFieldCharset = 3,
    TransportFieldPrefix = 4,
    SeedIdLength = 5,
    SeedIdCharset = 6,
    SeedLength = 7,
    CounterOverflow = 8,
    UnknownSeed = 9,
    NoRetainedReceipt = 10,
    ReplayRejected = 11,
    Mismatch = 12,
    Storage = 13,
    Transport = 14,
    Panic = 98,
    Unknown = 99,
}

impl From<&McpixError> for McpixStatus {
    fn from(value: &McpixError) -> Self {
        match value {
            McpixError::TransportFieldLength(_) => McpixStatus::TransportFieldLength,
            McpixError::TransportFieldCharset(_) => McpixStatus::TransportFieldCharset,
            McpixError::TransportFieldPrefix => McpixStatus::TransportFieldPrefix,
            McpixError::SeedIdLength { .. } => McpixStatus::SeedIdLength,
            McpixError::SeedIdCharset => McpixStatus::SeedIdCharset,
            McpixError::SeedLength { .. } => McpixStatus::SeedLength,
            McpixError::CounterOverflow => McpixStatus::CounterOverflow,
            McpixError::UnknownSeed => McpixStatus::UnknownSeed,
            McpixError::NoRetainedReceipt => McpixStatus::NoRetainedReceipt,
            McpixError::ReplayRejected => McpixStatus::ReplayRejected,
            McpixError::Mismatch => McpixStatus::Mismatch,
            McpixError::Storage(_) => McpixStatus::Storage,
            McpixError::Transport(_) => McpixStatus::Transport,
        }
    }
}
