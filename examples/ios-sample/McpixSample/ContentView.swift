// View mínima exercitando a SDK. Botão dispara o flow; Text mostra
// a saída numa fonte monospace para preservar alinhamento.

import SwiftUI
import MCPixSDK

struct ContentView: View {
    @State private var output: String = "Toque no botão para iniciar."
    @State private var running: Bool = false

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("mcpix-sdk demo")
                .font(.title)
                .fontWeight(.bold)

            Button(action: runDemo) {
                Text("Rodar fluxo: register → generate → validate")
                    .frame(maxWidth: .infinity)
                    .padding()
                    .background(running ? Color.gray : Color.blue)
                    .foregroundColor(.white)
                    .cornerRadius(8)
            }
            .disabled(running)

            ScrollView {
                Text(output)
                    .font(.system(.footnote, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding()
    }

    private func runDemo() {
        running = true
        output = "Iniciando…"

        // SDK calls são sync; rodar fora do main thread evita stall
        // na UI. Em produção, prefira `async` quando UniFFI suportar
        // future-aware Swift bindings.
        DispatchQueue.global(qos: .userInitiated).async {
            var lines: [String] = []
            do {
                let receiver = McpixReceiver()
                let seedId = "RECVR1"
                try receiver.register(seedId: seedId)
                lines.append("✓ recebedor cadastrado: SeedId=\(seedId)")

                let charge = try receiver.generateCharge(
                    seedId: seedId, amountCents: 9900
                )
                lines.append("✓ cobrança gerada:")
                lines.append("    transport (público):")
                lines.append("    \(charge.transportField)")
                lines.append("    counter T: \(charge.counter)")

                // C₂ deliberadamente errado para mostrar a defesa
                // anti-tampering. Em produção, o C₂ correto chega do
                // banco do pagador via HTTP mTLS.
                let outcome = try receiver.validateReceipt(
                    seedId: seedId, counter: charge.counter, presentedC2: "AAAAAAAAAAA"
                )
                lines.append("")
                lines.append("✓ validação com C₂ errado:")
                lines.append("    outcome: \(outcome)")
                lines.append("    (esperado mismatch — defesa anti-tampering)")
            } catch {
                lines.append("")
                lines.append("✗ erro: \(error)")
            }

            DispatchQueue.main.async {
                output = lines.joined(separator: "\n")
                running = false
            }
        }
    }
}

#Preview {
    ContentView()
}
