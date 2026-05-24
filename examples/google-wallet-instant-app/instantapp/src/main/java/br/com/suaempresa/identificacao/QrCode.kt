package br.com.suaempresa.identificacao

import android.graphics.Bitmap
import android.graphics.Color
import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter

/**
 * Renderiza o transport_field como QR Bitmap via ZXing (core ~100 KB,
 * cabe no orçamento de 15 MB do Instant App).
 *
 * O input é o transport_field do mcpix (35 chars ASCII), não um BR
 * Code do Banco Central. O terminal do pagador reconhece o prefixo
 * PIXOFFv1 e roteia para o fluxo mcpix.
 */
fun generateQrBitmap(content: String, sizePx: Int = 512): Bitmap {
    val matrix = QRCodeWriter().encode(content, BarcodeFormat.QR_CODE, sizePx, sizePx)
    val bmp = Bitmap.createBitmap(sizePx, sizePx, Bitmap.Config.RGB_565)
    for (x in 0 until sizePx) {
        for (y in 0 until sizePx) {
            bmp.setPixel(x, y, if (matrix[x, y]) Color.BLACK else Color.WHITE)
        }
    }
    return bmp
}
