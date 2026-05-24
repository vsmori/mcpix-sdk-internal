// Demo Kotlin JVM CLI — espelha o fluxo do recebedor através do binding
// UniFFI. A diferença em relação ao SmokeTest em bindings/kotlin é que
// este é um app standalone (gradle run), não uma asserção de teste.
//
// Para um app Android (UI completa), ver examples/android-sample/ —
// mesmo binding, embrulhado numa Activity.

import uniffi.mcpix_uniffi.McpixReceiver
import uniffi.mcpix_uniffi.McpixValidation

fun main() {
    println("=== mcpix-sdk — demo integrador Kotlin (JVM) ===\n")

    // (1) Instancia o receiver. `use { }` garante que `close()` seja
    //     chamado mesmo em exceções — libera o handle nativo via JNA.
    McpixReceiver().use { receiver ->
        // (2) Cadastra um SeedId. A Seed (32 bytes random) é gerada
        //     internamente pela SDK via OsRng.
        val seedId = "RECVR1"
        receiver.register(seedId)
        println("✓ recebedor cadastrado: SeedId=$seedId")

        // (3) Gera uma cobrança de R$ 99,00 (9900 centavos).
        val charge = receiver.generateCharge(seedId, 9900u)
        println("✓ cobrança gerada:")
        println("    transport field (público): ${charge.transportField}")
        println("    counter T:                  ${charge.counter}")
        println("    layout: 8 (prefix) + 16 (SeedId padded) + 11 (C₁) = 35 chars")

        // (4) Valida com C₂ deliberadamente errado para exercitar o
        //     caminho Mismatch. Em produção, o C₂ correto chega via
        //     resposta do banco do pagador (HTTP mTLS, fora do binding).
        val wrongC2 = "AAAAAAAAAAA" // 11 chars; alfabeto válido mas
                                     // não bate com o retido.
        val outcome = receiver.validateReceipt(seedId, charge.counter, wrongC2)
        println("\n✓ validação com C₂ errado:")
        println("    outcome: $outcome  (esperado: MISMATCH — defesa anti-tampering)")

        check(outcome == McpixValidation.MISMATCH) {
            "esperava MISMATCH, recebeu $outcome"
        }

        println("\n--- demo completo. Próximos passos para integração real: ---")
        println("  • C₂ correto vem do banco do pagador via HTTP mTLS")
        println("    (lookup_seed → apply_recover_c2). Ver docs/PROTOCOL.md.")
        println("  • Em Android: empacote o .aar de bindings/kotlin/aar/")
        println("    e use a mesma API a partir da Activity (ver examples/android-sample/).")
        println("  • Persistência custom: implemente SeedStore em Rust e exponha via UniFFI.")
    }
}
