// Renderização do transport_field como QR Code via CoreImage —
// framework nativo, sem dep de terceiros (importante pelo limite de
// 15-50 MB do App Clip, spec §5.2).
//
// A spec usa CIQRCodeGenerator para o Pix dinâmico; aqui o input do
// gerador é o `transport_field` do mcpix (35 chars ASCII), não um
// payload BR Code do Banco Central. Quem lê (terminal do pagador)
// reconhece o prefixo PIXOFFv1 e roteia para o fluxo mcpix.

import SwiftUI
import CoreImage.CIFilterBuiltins

struct BarcodeView: View {
    let transportField: String
    let counter: UInt64

    private let context = CIContext()

    var body: some View {
        VStack(spacing: 12) {
            if let image = qrImage(from: transportField) {
                Image(uiImage: image)
                    .interpolation(.none) // QR nítido, sem anti-alias
                    .resizable()
                    .scaledToFit()
                    .frame(width: 240, height: 240)
                    .accessibilityLabel("QR Code Pix offline")
            } else {
                Text("Falha ao gerar QR").foregroundColor(.red)
            }

            Text(transportField)
                .font(.system(.caption, design: .monospaced))
                .textSelection(.enabled)
                .multilineTextAlignment(.center)

            Text("T = \(counter)  ·  PIXOFFv1  ·  gerado offline")
                .font(.caption2)
                .foregroundColor(.secondary)

            Button {
                UIPasteboard.general.string = transportField
            } label: {
                Label("Pix Copia e Cola", systemImage: "doc.on.doc")
            }
            .buttonStyle(.bordered)
        }
    }

    private func qrImage(from string: String) -> UIImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(string.utf8)
        filter.correctionLevel = "M" // 15% — equilíbrio densidade/robustez
        guard let output = filter.outputImage else { return nil }
        // Escala para nitidez (CIQRCode sai ~27px; ampliamos 10x).
        let scaled = output.transformed(by: CGAffineTransform(scaleX: 10, y: 10))
        guard let cg = context.createCGImage(scaled, from: scaled.extent) else { return nil }
        return UIImage(cgImage: cg)
    }
}
