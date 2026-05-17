//! Camada FFI (Bloco 2 da especificação).
//!
//! Expõe uma C-ABI estável (`extern "C"`) que será consumida por:
//! - Swift via XCFramework (assinaturas C diretas + header gerado)
//! - Kotlin/Java via JNI (UniFFI será adicionado na próxima sessão)
//! - C# via P/Invoke (`DllImport`/`LibraryImport`)
//!
//! Garantias desta camada:
//! 1. **Nenhum panic atravessa a fronteira.** Toda função pública roda dentro
//!    de `catch_unwind`. Pânico → código de erro.
//! 2. **Sem alocação implícita do lado caller.** Strings vão e voltam como
//!    `*const c_char` UTF-8 com tamanho explícito; objetos com tempo de vida
//!    longo são `*mut OpaqueHandle`.
//! 3. **Free explícito.** Toda alocação retornada à plataforma vem com a
//!    função `*_free` correspondente. Ver `error.rs`.

#![deny(rust_2018_idioms)]

pub mod error;
mod handle;
mod receiver_api;
mod strings;

pub use error::McpixStatus;
pub use receiver_api::*;
