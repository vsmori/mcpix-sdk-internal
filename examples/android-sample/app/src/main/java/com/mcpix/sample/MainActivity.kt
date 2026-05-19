package com.mcpix.sample

import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import com.mcpix.sample.databinding.ActivityMainBinding
import uniffi.mcpix_uniffi.McpixReceiver
import uniffi.mcpix_uniffi.McpixValidation
import kotlin.concurrent.thread

/**
 * UI mínima exercitando a SDK no caminho Android.
 *
 * O flow rodando aqui é exatamente o mesmo do
 * `examples/kotlin-jvm-sample` — register → generateCharge →
 * validateReceipt — só que dentro de uma Activity, com saída numa
 * TextView e o trabalho jogado para uma thread para não bloquear o
 * main thread (a SDK por enquanto é sync; uma chamada não-bloqueante
 * viria via callback interface UniFFI).
 */
class MainActivity : AppCompatActivity() {

    private lateinit var binding: ActivityMainBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        binding.btnRun.setOnClickListener { runDemo() }
    }

    private fun runDemo() {
        binding.btnRun.isEnabled = false
        binding.output.text = "Iniciando…"

        // SDK é sync. Jogar para thread separada evita ANR; em produção
        // use coroutines (Dispatchers.IO).
        thread {
            val log = StringBuilder()
            try {
                McpixReceiver().use { receiver ->
                    val seedId = "RECVR1"
                    receiver.register(seedId)
                    log.appendLine("✓ recebedor cadastrado: SeedId=$seedId")

                    val charge = receiver.generateCharge(seedId, 9900u)
                    log.appendLine("✓ cobrança gerada:")
                    log.appendLine("    transport (público):")
                    log.appendLine("    ${charge.transportField}")
                    log.appendLine("    counter T: ${charge.counter}")

                    // C₂ deliberadamente errado para mostrar a defesa
                    // anti-tampering. Em produção, o C₂ válido chega
                    // via banco do pagador (HTTP mTLS).
                    val outcome = receiver.validateReceipt(
                        seedId, charge.counter, "AAAAAAAAAAA"
                    )
                    log.appendLine("\n✓ validação com C₂ errado:")
                    log.appendLine("    outcome: $outcome")
                    log.appendLine("    (esperado MISMATCH — defesa anti-tampering)")

                    check(outcome == McpixValidation.MISMATCH)
                    log.appendLine("\n✓ demo OK.")
                }
            } catch (e: Throwable) {
                log.appendLine("\n✗ erro: $e")
            }

            // Volta para main thread para atualizar UI.
            runOnUiThread {
                binding.output.text = log.toString()
                binding.btnRun.isEnabled = true
            }
        }
    }
}
