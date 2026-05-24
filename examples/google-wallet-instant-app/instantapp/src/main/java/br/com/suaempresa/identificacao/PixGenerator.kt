package br.com.suaempresa.identificacao

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import uniffi.mcpix_uniffi.McpixCharge
import uniffi.mcpix_uniffi.McpixReceiver
import java.net.HttpURLConnection
import java.net.URL

/**
 * Ponte entre o Instant App e o binding Kotlin do mcpix.
 *
 * Substitui a chamada por-transação à API Pix do Banco Central (spec
 * §4 passo 3) por geração offline: resgata a Seed UMA vez no init,
 * depois cada cobrança é local.
 *
 * Usa HttpURLConnection nativo (sem Retrofit/OkHttp) para respeitar
 * o limite de 15 MB do Instant App (spec §5).
 */
class PixGenerator {

    private var receiver: McpixReceiver? = null
    private var seedId: String = ""
    var holderName: String = ""
        private set

    /** Resposta do fetch de init: nome do titular + Seed selada. */
    private data class InitInfo(
        val holderName: String,
        val seedId: String,
        val sealedSeedBackup: String,
    )

    /**
     * Init: fetch backend → restore Seed → McpixReceiver pronto.
     * `passphrase` em produção vem de BiometricPrompt + Keystore, não
     * de input manual.
     */
    suspend fun bootstrap(memberId: String, passphrase: String) {
        val info = fetchInit(memberId)
        // fromSealedBackup é exposto pelo binding UniFFI (mcpix-uniffi):
        // decifra Argon2id+AEAD, registra a Seed, semeia o counter com
        // o T restaurado (próxima cobrança usa T+1).
        receiver = McpixReceiver.fromSealedBackup(info.sealedSeedBackup, passphrase)
        seedId = info.seedId
        holderName = info.holderName
    }

    /** Gera uma cobrança OFFLINE. Zero rede. */
    fun generate(amountCents: ULong): McpixCharge {
        val r = receiver ?: error("terminal não inicializado")
        return r.generateCharge(seedId, amountCents)
    }

    private suspend fun fetchInit(memberId: String): InitInfo = withContext(Dispatchers.IO) {
        // Em produção use mTLS (OkHttp + client cert) para o canal
        // banco↔terminal. Aqui HttpURLConnection nativo (mais leve).
        val url = URL("https://instantapp.suaempresa.com.br/init?memberId=$memberId")
        val conn = (url.openConnection() as HttpURLConnection).apply {
            requestMethod = "GET"
            connectTimeout = 5_000
            readTimeout = 5_000
        }
        try {
            check(conn.responseCode == 200) { "backend retornou ${conn.responseCode}" }
            val body = conn.inputStream.bufferedReader().readText()
            val json = JSONObject(body)
            InitInfo(
                holderName = json.getString("holderName"),
                seedId = json.getString("seedId"),
                sealedSeedBackup = json.getString("sealedSeedBackup"),
            )
        } finally {
            conn.disconnect()
        }
    }
}
