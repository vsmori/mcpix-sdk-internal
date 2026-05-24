package br.com.suaempresa.identificacao

import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent

/**
 * Activity efêmera do Instant App. Captura o `?id=` do App Link
 * (exatamente como a spec §4.1 descreve) e injeta na tela do terminal.
 *
 * O `id` é a CHAVE DE IDENTIFICAÇÃO do cartão Google Wallet (ex.
 * "SEC-99281-X"). NÃO é o SeedId do mcpix — o backend mapeia
 * memberId → SeedId provisionado no fetch de inicialização (ver
 * PixGenerator.kt).
 */
class PixCheckoutActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val data: Uri? = intent.data
        // Segurança (spec §5 + App Link): o `id` é só hint de QUAL
        // conta resgatar — não autoriza nada. A autorização real é a
        // posse da Seed, restaurada do backup selado no init (gated
        // por passphrase / Keystore). App Link com autoVerify garante
        // que só este app intercepta o link.
        val memberId = data?.getQueryParameter("id").orEmpty()

        setContent {
            PixTerminalScreen(memberId = memberId)
        }
    }
}
