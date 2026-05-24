// Envio do transport_field por NFC ao encostar no terminal do pagador.
//
// Modelo: o App Clip do recebedor escreve o `transport_field` num NDEF
// message; o terminal do pagador (que está em modo leitura NFC) o lê
// ao aproximar. Usa CoreNFC — framework nativo.
//
// ⚠️ CAVEAT App Clip + NFC (ver README §"NFC"):
//   - CoreNFC exige o entitlement
//     `com.apple.developer.nfc.readersession.formats`.
//   - App Clips suportam um SUBSET de entitlements. NFC tag *reading*
//     é suportado; peer-to-peer NDEF *writing* tem suporte que varia
//     por device/iOS. Confirme antes de depender só do NFC.
//   - O QR óptico (BarcodeView) é o caminho primário robusto; o NFC
//     é conveniência "tap" complementar.
//
// Este arquivo modela o lado RECEBEDOR escrevendo num NDEF tag/peer.
// Em iOS, escrita NDEF é feita sobre um `NFCNDEFTag` descoberto numa
// `NFCNDEFReaderSession` — o "tag" pode ser outro device em modo
// emulação ou um NFC sticker reprogramável no terminal do pagador.

import CoreNFC

final class NFCBeam: NSObject, NFCNDEFReaderSessionDelegate {
    private var session: NFCNDEFReaderSession?
    private var payload: String = ""
    private var onResult: ((Result<Void, Error>) -> Void)?

    /// Inicia uma sessão NFC e escreve `transportField` no primeiro
    /// tag NDEF aproximado.
    func beam(transportField: String, completion: @escaping (Result<Void, Error>) -> Void) {
        guard NFCNDEFReaderSession.readingAvailable else {
            completion(.failure(NFCError.unavailable))
            return
        }
        self.payload = transportField
        self.onResult = completion
        let session = NFCNDEFReaderSession(
            delegate: self,
            queue: nil,
            invalidateAfterFirstRead: true
        )
        session.alertMessage = "Aproxime do terminal de pagamento"
        session.begin()
        self.session = session
    }

    // MARK: - NFCNDEFReaderSessionDelegate

    func readerSession(_ session: NFCNDEFReaderSession, didDetect tags: [NFCNDEFTag]) {
        guard let tag = tags.first else { return }
        session.connect(to: tag) { [weak self] error in
            guard let self else { return }
            if let error {
                self.finish(session, .failure(error))
                return
            }
            // Monta um NDEF message com um único record de texto
            // carregando o transport_field. O leitor (pagador)
            // reconhece o prefixo PIXOFFv1 e roteia para o fluxo mcpix.
            let textPayload = NFCNDEFPayload.wellKnownTypeTextPayload(
                string: self.payload,
                locale: Locale(identifier: "en")
            )
            guard let textPayload else {
                self.finish(session, .failure(NFCError.encodingFailed))
                return
            }
            let message = NFCNDEFMessage(records: [textPayload])
            tag.writeNDEF(message) { writeError in
                if let writeError {
                    self.finish(session, .failure(writeError))
                } else {
                    self.finish(session, .success(()))
                }
            }
        }
    }

    func readerSession(_ session: NFCNDEFReaderSession, didInvalidateWithError error: Error) {
        // Sessão encerrada (timeout, cancelamento, ou após escrita).
        // Se ainda não reportamos resultado, propaga o erro.
        if onResult != nil {
            finish(session, .failure(error))
        }
    }

    // Required pela delegate, mas no fluxo de escrita não usamos a
    // leitura de mensagens — só descoberta de tags acima.
    func readerSession(_ session: NFCNDEFReaderSession, didDetectNDEFs messages: [NFCNDEFMessage]) {}

    private func finish(_ session: NFCNDEFReaderSession, _ result: Result<Void, Error>) {
        let cb = onResult
        onResult = nil
        if case .success = result {
            session.alertMessage = "Enviado ao terminal."
        }
        session.invalidate()
        cb?(result)
    }
}

enum NFCError: Error {
    case unavailable
    case encodingFailed
}
