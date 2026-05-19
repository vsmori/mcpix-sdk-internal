//! API C-ABI do `ReceiverSdk`.
//!
//! Esta é a superfície que Swift/Kotlin/.NET vai chamar. Mantemos o conjunto
//! deliberadamente pequeno (5 funções) para reduzir a área de manutenção da
//! ABI.

use std::os::raw::c_char;
use std::sync::Arc;

use mcpix_core::state::ValidationOutcome;
use mcpix_core::types::SeedId;
use mcpix_receiver_sdk::{
    memory_store::InMemorySeedStore, monotonic_counter::InMemoryCounter, system_random::OsRandom,
    ReceiverSdk,
};

use crate::error::McpixStatus;
use crate::handle::{guard, guard_mut};
use crate::strings::{cstr_to_str, string_to_cstr};

// Re-export para que `mcpix_string_free` exista no namespace público da crate
// e os bindings encontrem o símbolo.
pub use crate::strings::mcpix_string_free;

/// Handle opaco devolvido por `mcpix_receiver_new`. Tratá-lo como tipo
/// alheio do lado nativo — apenas passar e liberar.
#[repr(C)]
pub struct McpixReceiver {
    _private: [u8; 0],
}

/// Resultado de validação devolvido por `mcpix_receiver_validate`. Espelha
/// `ValidationOutcome` com layout C estável.
#[repr(i32)]
pub enum McpixValidation {
    Valid = 0,
    Mismatch = 1,
    Replay = 2,
}

impl From<ValidationOutcome> for McpixValidation {
    fn from(v: ValidationOutcome) -> Self {
        match v {
            ValidationOutcome::Valid => McpixValidation::Valid,
            ValidationOutcome::Mismatch => McpixValidation::Mismatch,
            ValidationOutcome::Replay => McpixValidation::Replay,
        }
    }
}

/// Cria um novo `ReceiverSdk` com store/contador/RNG default (in-memory + OS).
///
/// Versões futuras receberão delegates injetáveis (HttpTransport, SeedStore
/// custom) seguindo o padrão da seção 3 da especificação.
///
/// # Safety
/// `out_handle` precisa apontar para um `*mut McpixReceiver` válido.
#[no_mangle]
pub unsafe extern "C" fn mcpix_receiver_new(out_handle: *mut *mut McpixReceiver) -> McpixStatus {
    guard(|| {
        if out_handle.is_null() {
            return McpixStatus::InvalidArgument;
        }
        let sdk = ReceiverSdk::new(
            Arc::new(InMemorySeedStore::new()),
            Arc::new(InMemoryCounter::new()),
            Arc::new(OsRandom),
        );
        let boxed = Box::new(sdk);
        *out_handle = Box::into_raw(boxed) as *mut McpixReceiver;
        McpixStatus::Ok
    })
}

/// Libera o handle. No-op se `handle` for nulo.
///
/// # Safety
/// `handle` deve ter sido produzido por `mcpix_receiver_new` e não pode ter
/// sido liberado anteriormente.
#[no_mangle]
pub unsafe extern "C" fn mcpix_receiver_free(handle: *mut McpixReceiver) {
    if handle.is_null() {
        return;
    }
    let _ = Box::from_raw(handle as *mut ReceiverSdk);
}

/// Registra um recebedor. `seed_id` é UTF-8 terminado em null.
///
/// # Safety
/// `handle` deve ser válido. `seed_id` deve apontar para C-string UTF-8.
#[no_mangle]
pub unsafe extern "C" fn mcpix_receiver_register(
    handle: *mut McpixReceiver,
    seed_id: *const c_char,
) -> McpixStatus {
    if handle.is_null() {
        return McpixStatus::InvalidArgument;
    }
    let sdk = &*(handle as *const ReceiverSdk);
    guard_mut(|| {
        let seed_id_str = match cstr_to_str(seed_id) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let seed_id = match SeedId::new(seed_id_str.to_string()) {
            Ok(s) => s,
            Err(e) => return McpixStatus::from(&e),
        };
        match sdk.register(seed_id) {
            Ok(_) => McpixStatus::Ok,
            Err(e) => McpixStatus::from(&e),
        }
    })
}

/// Gera uma cobrança. Em sucesso, devolve o campo de transporte público em
/// `*out_field` (deve ser liberado com `mcpix_string_free`) e o contador
/// usado em `*out_counter`.
///
/// # Safety
/// Todos os ponteiros devem ser válidos. `seed_id` UTF-8 terminado em null.
#[no_mangle]
pub unsafe extern "C" fn mcpix_receiver_generate_charge(
    handle: *mut McpixReceiver,
    seed_id: *const c_char,
    amount_cents: u64,
    out_field: *mut *mut c_char,
    out_counter: *mut u64,
) -> McpixStatus {
    if handle.is_null() || out_field.is_null() || out_counter.is_null() {
        return McpixStatus::InvalidArgument;
    }
    let sdk = &*(handle as *const ReceiverSdk);
    guard_mut(|| {
        let seed_id_str = match cstr_to_str(seed_id) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let seed_id = match SeedId::new(seed_id_str.to_string()) {
            Ok(s) => s,
            Err(e) => return McpixStatus::from(&e),
        };
        match sdk.generate_charge(&seed_id, amount_cents) {
            Ok(charge) => {
                let cstr = string_to_cstr(&charge.transport_field);
                if cstr.is_null() {
                    return McpixStatus::Unknown;
                }
                *out_field = cstr;
                *out_counter = charge.counter;
                McpixStatus::Ok
            }
            Err(e) => McpixStatus::from(&e),
        }
    })
}

/// Valida um C₂ apresentado. Em sucesso, escreve o resultado em `*out_result`.
///
/// # Safety
/// Todos os ponteiros devem ser válidos. `seed_id` e `presented_c2` em UTF-8.
#[no_mangle]
pub unsafe extern "C" fn mcpix_receiver_validate(
    handle: *mut McpixReceiver,
    seed_id: *const c_char,
    counter: u64,
    presented_c2: *const c_char,
    out_result: *mut i32,
) -> McpixStatus {
    if handle.is_null() || out_result.is_null() {
        return McpixStatus::InvalidArgument;
    }
    let sdk = &*(handle as *const ReceiverSdk);
    guard_mut(|| {
        let seed_id_str = match cstr_to_str(seed_id) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let presented = match cstr_to_str(presented_c2) {
            Ok(s) => s,
            Err(e) => return e,
        };
        let seed_id = match SeedId::new(seed_id_str.to_string()) {
            Ok(s) => s,
            Err(e) => return McpixStatus::from(&e),
        };
        match sdk.validate_receipt(&seed_id, counter, presented) {
            Ok(outcome) => {
                let v: McpixValidation = outcome.into();
                *out_result = v as i32;
                McpixStatus::Ok
            }
            Err(e) => McpixStatus::from(&e),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn cstr(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    #[test]
    fn ffi_full_flow() {
        unsafe {
            let mut handle: *mut McpixReceiver = std::ptr::null_mut();
            assert_eq!(mcpix_receiver_new(&mut handle as *mut _), McpixStatus::Ok);
            assert!(!handle.is_null());

            let sid = cstr("R1");
            assert_eq!(
                mcpix_receiver_register(handle, sid.as_ptr()),
                McpixStatus::Ok
            );

            let mut field: *mut c_char = std::ptr::null_mut();
            let mut counter: u64 = 0;
            assert_eq!(
                mcpix_receiver_generate_charge(
                    handle,
                    sid.as_ptr(),
                    9900,
                    &mut field as *mut _,
                    &mut counter as *mut _,
                ),
                McpixStatus::Ok
            );
            assert!(!field.is_null());
            assert!(counter > 0);

            mcpix_string_free(field);
            mcpix_receiver_free(handle);
        }
    }
}
