// Terminal Pix do App Clip. Fluxo de telas (spec §4):
//   1. Loading — fetch backend (valida chave + resgata Seed selada)
//   2. Input — teclado numérico para o valor (R$ 0,00)
//   3. Geração — generateCharge() OFFLINE → transport_field
//   4. Exibição — QR + Copia-e-Cola + botão "Enviar por NFC"

import SwiftUI

struct ContentView: View {
    @Binding var memberId: String
    @StateObject private var generator = PixGenerator()
    @State private var amountText: String = ""
    @State private var charge: (transportField: String, counter: UInt64)?
    @State private var errorText: String?

    private let nfc = NFCBeam()

    var body: some View {
        NavigationStack {
            content
                .padding()
                .navigationTitle("Recebimento Pix")
        }
        .task(id: memberId) {
            guard !memberId.isEmpty else { return }
            // Em produção, a passphrase vem de Face ID / Keychain
            // protegido por biometria — não hardcoded.
            await generator.bootstrap(memberId: memberId, passphrase: "demo-passphrase")
        }
    }

    @ViewBuilder
    private var content: some View {
        switch generator.state {
        case .loading:
            ProgressView("Validando chave \(memberId)…")

        case let .failed(msg):
            VStack(spacing: 12) {
                Image(systemName: "exclamationmark.triangle").font(.largeTitle)
                Text(msg).multilineTextAlignment(.center)
            }
            .foregroundColor(.red)

        case let .ready(holderName, seedId):
            if let charge {
                resultView(charge: charge)
            } else {
                inputView(holderName: holderName, seedId: seedId)
            }
        }
    }

    // Etapa 2 — input do valor.
    private func inputView(holderName: String, seedId: String) -> some View {
        VStack(spacing: 20) {
            VStack(spacing: 4) {
                Text("Recebedor").font(.caption).foregroundColor(.secondary)
                Text(holderName).font(.title2).fontWeight(.semibold)
                Text(seedId).font(.caption2).foregroundColor(.secondary)
            }

            TextField("R$ 0,00", text: $amountText)
                .keyboardType(.decimalPad)
                .font(.system(size: 36, weight: .bold, design: .rounded))
                .multilineTextAlignment(.center)
                .textFieldStyle(.roundedBorder)

            Button {
                generateCharge()
            } label: {
                Text("Gerar QR Code").frame(maxWidth: .infinity).padding(8)
            }
            .buttonStyle(.borderedProminent)
            .disabled(centsFromInput() == nil)

            if let errorText {
                Text(errorText).foregroundColor(.red).font(.caption)
            }

            Text("Gerado offline — sem chamada ao PSP por transação.")
                .font(.caption2).foregroundColor(.secondary)
        }
    }

    // Etapa 4 — exibição do QR + ações.
    private func resultView(charge: (transportField: String, counter: UInt64)) -> some View {
        VStack(spacing: 20) {
            BarcodeView(transportField: charge.transportField, counter: charge.counter)

            Button {
                nfc.beam(transportField: charge.transportField) { result in
                    if case let .failure(err) = result {
                        errorText = "NFC: \(err)"
                    }
                }
            } label: {
                Label("Enviar por NFC", systemImage: "wave.3.right")
                    .frame(maxWidth: .infinity).padding(8)
            }
            .buttonStyle(.bordered)

            Button("Nova cobrança") {
                self.charge = nil
                self.amountText = ""
            }
            .font(.callout)
        }
    }

    // MARK: - Helpers

    private func centsFromInput() -> UInt64? {
        // Aceita "12,34" ou "12.34" → 1234 centavos.
        let normalized = amountText.replacingOccurrences(of: ",", with: ".")
        guard let value = Double(normalized), value > 0 else { return nil }
        return UInt64((value * 100).rounded())
    }

    private func generateCharge() {
        errorText = nil
        guard let cents = centsFromInput() else {
            errorText = "Valor inválido"
            return
        }
        do {
            charge = try generator.generate(amountCents: cents)
        } catch {
            errorText = "Falha ao gerar: \(error)"
        }
    }
}
