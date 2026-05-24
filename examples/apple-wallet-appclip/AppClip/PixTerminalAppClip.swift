// Entry point do App Clip. Captura o `?id=` do Universal Link via
// NSUserActivity (exatamente como a spec descreve no item 4.1) e
// injeta no estado do terminal.
//
// O `id` é a CHAVE DE IDENTIFICAÇÃO do cartão Wallet (ex.
// "SEC-99281-X"). Ele NÃO é o SeedId do mcpix — o backend mapeia
// memberId → SeedId provisionado no fetch de inicialização (ver
// PixGenerator.swift).

import SwiftUI

@main
struct PixTerminalAppClip: App {
    @State private var memberId: String = ""

    var body: some Scene {
        WindowGroup {
            ContentView(memberId: $memberId)
                .onContinueUserActivity(NSUserActivityTypeBrowsingWeb) { userActivity in
                    guard
                        let incomingURL = userActivity.webpageURL,
                        let components = URLComponents(
                            url: incomingURL,
                            resolvingAgainstBaseURL: true
                        ),
                        let queryItems = components.queryItems
                    else { return }

                    if let idValue = queryItems.first(where: { $0.name == "id" })?.value {
                        // Inicializa o terminal com a chave capturada.
                        // Segurança (spec §5.1): o `id` é apenas um hint
                        // de QUAL conta resgatar — não autoriza nada por
                        // si só. A autorização real é a posse da Seed,
                        // restaurada do backup selado no init (que exige
                        // passphrase / chave hw-bound do titular).
                        self.memberId = idValue
                    }
                }
        }
    }
}
