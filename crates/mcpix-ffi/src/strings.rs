//! Conversão segura de strings na fronteira C.
//!
//! Convenções:
//! - Entrada: `*const c_char` UTF-8 terminado em null. Convertemos com `CStr`.
//! - Saída: `*mut c_char` alocado pelo Rust, liberado por `mcpix_string_free`.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use crate::error::McpixStatus;

/// Converte `ptr` em `&str`. Retorna `Err(McpixStatus::InvalidArgument)` se
/// `ptr` for nulo ou se o conteúdo não for UTF-8 válido.
pub(crate) fn cstr_to_str<'a>(ptr: *const c_char) -> Result<&'a str, McpixStatus> {
    if ptr.is_null() {
        return Err(McpixStatus::InvalidArgument);
    }
    let cstr = unsafe { CStr::from_ptr(ptr) };
    cstr.to_str().map_err(|_| McpixStatus::InvalidArgument)
}

/// Aloca uma string C a partir de `&str`. Retorna `null` em caso de OOM
/// improvável (interior NUL impossível porque nossos outputs são alfanuméricos).
pub(crate) fn string_to_cstr(s: &str) -> *mut c_char {
    match CString::new(s) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Libera uma string previamente alocada por esta FFI.
///
/// # Safety
/// `ptr` precisa ter sido devolvido por uma função desta crate. Passar `null`
/// é seguro (no-op).
#[no_mangle]
pub unsafe extern "C" fn mcpix_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    let _ = CString::from_raw(ptr);
}
