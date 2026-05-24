// App principal — host obrigatório do App Clip. A Apple exige que
// todo App Clip tenha um app completo associado, mesmo que o host
// seja mínimo. O fluxo real (terminal Pix) vive no App Clip; este
// host só existe para satisfazer o requisito e, em produção, seria
// o app bancário completo.

import SwiftUI

@main
struct McpixWalletHostApp: App {
    var body: some Scene {
        WindowGroup {
            VStack(spacing: 16) {
                Image(systemName: "creditcard.fill").font(.largeTitle)
                Text("MCPix Wallet — App Principal")
                    .font(.headline)
                Text("O terminal de recebimento Pix abre via App Clip ao tocar no link do cartão na Apple Wallet.")
                    .font(.callout)
                    .foregroundColor(.secondary)
                    .multilineTextAlignment(.center)
            }
            .padding()
        }
    }
}
