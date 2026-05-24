// Ponte entre o App Clip e o mcpix Swift binding.
//
// Responsabilidades:
//   1. No init: resgatar do backend a identidade do titular + a Seed
//      do recebedor selada (blob mcpix-backup) a partir do memberId.
//   2. Restaurar a Seed localmente (uma vez) → McpixReceiver pronto.
//   3. Por transação: generateCharge() OFFLINE → transport_field.
//
// É o passo que substitui a chamada por-transação à API do Banco
// Central (spec §4 passo 3): aqui, depois do init, NÃO há rede.

import Foundation
import MCPixSDK

/// Estado de inicialização do terminal.
enum TerminalState {
    case loading
    case ready(holderName: String, seedId: String)
    case failed(String)
}

/// Resposta do fetch de inicialização do backend. O backend valida o
/// memberId, e devolve o nome do titular + o blob de backup selado da
/// Seed do recebedor (formato mcpix-backup, Base58Check).
struct InitResponse: Decodable {
    let holderName: String
    let seedId: String          // SeedId mcpix provisionado (alfabeto válido)
    let sealedSeedBackup: String // blob mcpix-backup (Base58Check)
}

@MainActor
final class PixGenerator: ObservableObject {
    @Published var state: TerminalState = .loading

    private var receiver: McpixReceiver?
    private var seedId: String = ""

    /// Init: fetch backend → restore Seed → McpixReceiver pronto.
    ///
    /// `passphrase` desbloqueia o backup selado. Em produção, vem de
    /// Face ID / Touch ID (Keychain item protegido por biometria),
    /// não de input manual. Aqui é parâmetro para o exemplo.
    func bootstrap(memberId: String, passphrase: String) async {
        state = .loading
        do {
            // (1) fetch backend — mesma chamada que a spec já faz no
            //     passo 1 ("validar chave + resgatar nome"). Aqui ela
            //     ALSO devolve o backup selado da Seed.
            let info = try await fetchInit(memberId: memberId)

            // (2) restaura a Seed do backup selado e instancia o
            //     McpixReceiver. `fromSealedBackup` é exposto pelo
            //     binding UniFFI (mcpix-uniffi) — decifra o blob
            //     Argon2id+AEAD, registra a Seed e semeia o counter
            //     com o T restaurado (próxima cobrança usa T+1).
            let r = try McpixReceiver.fromSealedBackup(
                backup: info.sealedSeedBackup,
                passphrase: passphrase
            )

            self.receiver = r
            self.seedId = info.seedId
            state = .ready(holderName: info.holderName, seedId: info.seedId)
        } catch {
            state = .failed("Falha ao inicializar terminal: \(error)")
        }
    }

    /// Gera uma cobrança OFFLINE. Retorna o campo de transporte público
    /// de 35 chars + o counter T. Zero rede.
    func generate(amountCents: UInt64) throws -> (transportField: String, counter: UInt64) {
        guard let receiver else {
            throw TerminalError.notReady
        }
        let charge = try receiver.generateCharge(seedId: seedId, amountCents: amountCents)
        return (charge.transportField, charge.counter)
    }

    // MARK: - Backend fetch

    private func fetchInit(memberId: String) async throws -> InitResponse {
        // Em produção use mTLS (URLSession + URLSessionDelegate com
        // client cert) para o canal banco↔terminal. Aqui, esqueleto
        // HTTPS simples.
        var comps = URLComponents(string: "https://appclip.suaempresa.com.br/init")!
        comps.queryItems = [URLQueryItem(name: "memberId", value: memberId)]
        let (data, response) = try await URLSession.shared.data(from: comps.url!)
        guard let http = response as? HTTPURLResponse, http.statusCode == 200 else {
            throw TerminalError.backendUnavailable
        }
        return try JSONDecoder().decode(InitResponse.self, from: data)
    }
}

enum TerminalError: Error {
    case notReady
    case backendUnavailable
}
