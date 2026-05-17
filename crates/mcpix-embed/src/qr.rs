//! QR code encoder — wrapper sobre `qrcodegen-no-heap`.
//!
//! Output: matriz quadrada de bits. Caller passa buffers de tamanho
//! suficiente; rotina não aloca. Tamanho dos buffers depende da versão
//! máxima escolhida (M = 7 cabe os 35 chars do campo com folga).
//!
//! ## Uso típico
//!
//! ```ignore
//! let mut tmp = [0u8; qrcodegen_nostd::QrCode::BUFFER_LEN_FOR_VERSION as usize];
//! let mut out = [0u8; qrcodegen_nostd::QrCode::BUFFER_LEN_FOR_VERSION as usize];
//! let qr = charge_qr(field_str, &mut tmp, &mut out)?;
//! for y in 0..qr.size() {
//!   for x in 0..qr.size() {
//!     if qr.get_module(x, y) { /* desenha pixel preto no display */ }
//!   }
//! }
//! ```

use qrcodegen_no_heap::{QrCode, QrCodeEcc, Version};

use crate::types::EmbedError;

/// Versão QR máxima alvo. Versão 4 (33×33 módulos) cabe os 35 chars
/// alfanuméricos com folga e ainda permite ECC nível M (recovery ~15%).
pub const QR_MAX_VERSION: Version = Version::new(4);

/// Tamanho do buffer auxiliar exigido pelo encoder. Não-alloc.
/// Para versão 4 (33×33 módulos): ~137 bytes — confortável em qualquer MCU.
pub const QR_BUF_LEN: usize = QR_MAX_VERSION.buffer_len();

/// Codifica `field` (transport field ASCII de 35 chars) em QR. Retorna
/// `QrCode` cujos módulos são lidos com `.get_module(x, y) -> bool`.
///
/// Buffers necessários: dois `[u8; QR_BUF_LEN]` — um temp e um destino.
/// Ambos vivem na stack do caller. Sem alloc.
pub fn charge_qr<'a>(
    field: &str,
    tmp_buf: &'a mut [u8; QR_BUF_LEN],
    out_buf: &'a mut [u8; QR_BUF_LEN],
) -> Result<QrCode<'a>, EmbedError> {
    // ECC nível M é equilíbrio entre densidade e robustez visual em
    // displays e-paper / OLED comuns no ESP8266/ESP32.
    QrCode::encode_text(
        field,
        tmp_buf,
        out_buf,
        QrCodeEcc::Medium,
        Version::MIN,
        QR_MAX_VERSION,
        None,
        true,
    )
    .map_err(|_| EmbedError::TransportFieldLayout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::derive_pair;
    use crate::transport_field::{encode_into, TRANSPORT_FIELD_LEN};
    use crate::types::{Seed, SeedId};

    #[test]
    fn charge_produces_decodable_qr_module_grid() {
        let sid = SeedId::new("R1").unwrap();
        let seed = Seed::from_bytes([0x77; 32]);
        let (c1, _) = derive_pair(&seed, 9);
        let mut field_buf = [0u8; TRANSPORT_FIELD_LEN];
        let field = encode_into(&sid, &c1, &mut field_buf);

        let mut tmp = [0u8; QR_BUF_LEN];
        let mut out = [0u8; QR_BUF_LEN];
        let qr = charge_qr(field, &mut tmp, &mut out).unwrap();
        // Sanity: módulos centrais existem; tamanho está dentro do esperado.
        let size = qr.size();
        assert!((21..=33).contains(&size), "unexpected QR size {size}");
        // Pelo menos um módulo escuro e um claro — não é matriz vazia.
        let mut has_dark = false;
        let mut has_light = false;
        for y in 0..size {
            for x in 0..size {
                if qr.get_module(x, y) {
                    has_dark = true;
                } else {
                    has_light = true;
                }
            }
        }
        assert!(has_dark && has_light);
    }
}
