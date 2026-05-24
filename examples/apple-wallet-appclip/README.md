# Sample — Apple Wallet + App Clip com geração offline via mcpix

Implementa a arquitetura da spec "Apple Wallet & App Clip" (Generic
Pass + App Clip interativo para recebimento Pix), **substituindo a
chamada por-transação à API do Banco Central pela geração offline do
mcpix-sdk**.

⚠️ **Não validado neste ambiente** — App Clips exigem um projeto
Xcode com target type específico + entitlements + macOS. O código
aqui segue a API do binding Swift (`bindings/swift/`) e a spec
fornecida; build real exige Xcode 15+.

## A diferença que o mcpix introduz

A spec original (passo 3 "Geração do Pix") faz:

```
App Clip → backend da empresa → API Pix Banco Central (/v2/cob) → QR dinâmico
```

Isso é **uma chamada de rede por transação**. O mcpix inverte isso:

```
App Clip (com Seed restaurada) → generateCharge() offline → transport_field → QR
```

A `Seed` do recebedor é resgatada **uma vez** na inicialização do App
Clip (o mesmo fetch que a spec já faz no passo 1 para "validar se a
chave está ativa e resgatar o nome do titular"). Depois disso, **cada
cobrança é gerada localmente, sem rede** — é a "substituição
institucional offline" do protocolo.

| Aspecto | Spec original | Com mcpix |
|---|---|---|
| Rede por transação | sim (Banco Central /v2/cob) | **não** (só no init) |
| Funciona offline após init | não | **sim** |
| Latência de geração | RTT ao PSP | **~1 ms local** |
| Ponto de falha | API do PSP indisponível | nenhum (após init) |

## Fluxo completo

```
┌─ Apple Wallet (Generic Pass) ──────────────────────────┐
│  Cartão de identidade: TITULAR, CHAVE, foto (thumbnail)│
│  barcode PDF417 = identidade (SEC-99281-X|Nome)        │
│  backFields: link "RECEBER PAGAMENTO VIA PIX"          │
└───────────────────────────┬────────────────────────────┘
                            │ toque no link (Universal Link)
                            ▼
┌─ App Clip (PixTerminalAppClip) ────────────────────────┐
│  1. onContinueUserActivity → captura ?id=SEC-99281-X   │
│  2. fetch backend: valida chave + resgata Seed selada  │
│     (mcpix-backup blob) + nome do titular              │
│  3. restore local da Seed (mcpix-backup::import)       │
│  4. usuário digita valor (R$ 0,00)                     │
│  5. generateCharge(seedId, amountCents) → OFFLINE      │
│     → transport_field (35 chars) + counter T           │
│  6a. renderiza QR (CIQRCodeGenerator)                  │
│  6b. OU envia via NFC (CoreNFC NDEF) ao encostar no    │
│      terminal do pagador                               │
└────────────────────────────────────────────────────────┘
                            │ pagador lê QR/NFC → transport_field
                            ▼
       banco do pagador: lookup_seed + apply_recover_c2
       → C₂ → liquida → (opcional) push APNs de volta ao Wallet
```

## Estrutura de arquivos

```
apple-wallet-appclip/
├── README.md                    ← este arquivo
├── pass/
│   ├── pass.json                ← Generic Pass (adaptado da spec)
│   └── README.md                ← como assinar + gerar .pkpass
└── AppClip/
    ├── PixTerminalAppClip.swift ← @main + captura NSUserActivity
    ├── ContentView.swift        ← terminal: header + input + gerar
    ├── PixGenerator.swift       ← wrapper do mcpix Swift binding
    ├── BarcodeView.swift        ← QR + PDF417 via CoreImage
    └── NFCBeam.swift            ← CoreNFC: envia transport_field por NDEF
```

## Setup Xcode (resumo)

App Clips **não** compilam via `swift build` puro — precisam de um
projeto Xcode com:

1. **Target principal** (app completo) — host do App Clip.
2. **Target App Clip** com Bundle ID terminado em `.Clip`
   (`br.com.suaempresa.identificacao.Clip`).
3. **Associated Domains**: `appclips:appclip.suaempresa.com.br`.
4. **AASA file** hospedado em
   `https://appclip.suaempresa.com.br/.well-known/apple-app-site-association`
   (template em `pass/README.md`).
5. **mcpix XCFramework** como dependência do target App Clip:
   ```
   cargo xtask build-ios && cargo xtask package-xcframework
   ```
   Arrastar `bindings/swift/MCPixSDKFFI.xcframework` para o target.

### Restrição de tamanho do App Clip

A Apple impõe limite de 15 MB (iOS 15) a 50 MB (iOS 16+) para o
binário do App Clip. O `MCPixSDKFFI.xcframework` para um único
slice (arm64 device) é ~400 KB — confortável dentro do limite. O
custo está nas deps Swift de UI, que devemos manter mínimas
(só SwiftUI + CoreImage + CoreNFC, todas do sistema).

## NFC: enviar transport_field ao encostar

`AppClip/NFCBeam.swift` usa CoreNFC para escrever o `transport_field`
num NDEF message que o terminal do pagador lê ao encostar.

**Caveats de App Clip + NFC:**
- CoreNFC exige o entitlement `com.apple.developer.nfc.readersession.formats`.
  App Clips suportam um subset de entitlements — **NFC tag reading**
  é suportado; **NDEF tag writing** (peer emulation) tem suporte
  limitado e depende do device. Confirme no
  [App Clip entitlements list](https://developer.apple.com/documentation/app_clips)
  para o seu iOS target.
- Para phone-to-phone confiável, o caminho mais robusto continua
  sendo o **QR code óptico** (`BarcodeView.swift`). O NFC é uma
  conveniência "tap" complementar, não substituta.

## O que o mcpix cobre vs o que fica fora

| Componente | Coberto pelo mcpix | Responsabilidade do integrador |
|---|---|---|
| Gerar `transport_field` offline | ✅ `generateCharge` | — |
| Restaurar Seed do backup selado | ✅ `McpixReceiver.fromSealedBackup(backup:passphrase:)` (binding UniFFI) | passphrase / key management |
| Renderizar QR | — (CoreImage nativo) | `BarcodeView.swift` |
| Enviar por NFC | — (CoreNFC nativo) | `NFCBeam.swift` + entitlements |
| Validar comprovante (lado pagador) | ✅ no banco do pagador (`mcpix-bank-receiver`) | infra do PSP |
| Push APNs de sucesso ao Wallet | — | backend + APNs cert |

## Segurança (mapeando a §5 da spec)

1. **Adulteração do `?id=`**: a spec recomenda assinar/criptografar o
   parâmetro. No mcpix, mesmo que o atacante troque o `id`, ele só
   conseguiria gerar cobranças para uma Seed que **não possui** —
   o `generateCharge` exige a Seed real, restaurada do backup selado
   que só o titular legítimo consegue decifrar (passphrase / chave
   hw-bound). O `id` na URL vira só um *hint* de qual conta, não um
   token de autorização.
2. **Limite de 15-50 MB**: o XCFramework mcpix é ~400 KB/slice —
   dentro do orçamento.
3. **Push de sucesso**: ortogonal ao mcpix; backend faz APNs quando o
   banco do pagador confirma liquidação.
