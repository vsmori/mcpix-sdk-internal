package br.com.suaempresa.identificacao

import android.nfc.NdefMessage
import android.nfc.NdefRecord

/**
 * Empacota o transport_field num NDEF message para envio por NFC ao
 * terminal do pagador.
 *
 * Modelo de entrega no Android:
 *   - HCE (Host Card Emulation): o device do recebedor emula um tag
 *     NFC; o terminal do pagador (reader) lê ao aproximar. Exige um
 *     `HostApduService` declarado no manifest.
 *   - NDEF push direto: descontinuado (Android Beam removido no
 *     Android 10). HCE é o caminho atual.
 *
 * ⚠️ CAVEAT Instant App + NFC: `android.permission.NFC` é concedida
 * sem prompt, mas registrar um HostApduService a partir de um Instant
 * App tem limites de ciclo de vida. O caminho robusto continua sendo
 * o QR óptico (QrCode.kt); o NFC é conveniência "tap" complementar.
 *
 * Este helper só constrói o NDEF message; a integração com o
 * HostApduService (resposta a APDUs SELECT/READ) fica a cargo do
 * integrador conforme o protocolo do terminal do pagador.
 */
object NfcBeam {

    /** MIME type customizado que o terminal do pagador filtra. */
    private const val MIME_TYPE = "application/vnd.mcpix.transport"

    /**
     * Constrói o NDEF message carregando o transport_field como um
     * record MIME. O leitor (pagador) reconhece o MIME type e o
     * prefixo PIXOFFv1 do payload, roteando para o fluxo mcpix.
     */
    fun buildNdef(transportField: String): NdefMessage {
        val record = NdefRecord.createMime(
            MIME_TYPE,
            transportField.toByteArray(Charsets.US_ASCII),
        )
        return NdefMessage(arrayOf(record))
    }
}
