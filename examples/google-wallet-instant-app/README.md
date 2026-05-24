# Sample — Google Wallet + Play Instant com geração offline via mcpix

Equivalente Android do sample [`apple-wallet-appclip`](../apple-wallet-appclip/).
Implementa a spec "Google Wallet & Google Play Instant" (GenericObject
cloud-first + Instant App para recebimento Pix), **substituindo a
chamada por-transação à API Pix do Banco Central pela geração offline
do mcpix-sdk** — a mesma inversão de design do sample iOS.

⚠️ **Não validado neste ambiente** — Instant Apps exigem Android SDK +
Gradle + um App Bundle com módulo `instant`. O código segue o binding
Kotlin (`bindings/kotlin/`) e a spec; build real exige Android Studio.

## Apple vs Google — o que muda

| Aspecto | Apple (App Clip) | Google (Instant App) |
|---|---|---|
| Cartão | `.pkpass` local assinado | GenericObject **cloud-first** (REST API + JWT) |
| Mini-app | App Clip | Play Instant (`instant` module) |
| Link → app | Universal Link + AASA | App Link + `assetlinks.json` + `autoVerify` |
| Captura do `id` | `NSUserActivity` | `intent.data` em `onCreate` |
| UI | SwiftUI | Jetpack Compose |
| QR | CoreImage `CIQRCodeGenerator` | ZXing / `CIQRCodeGenerator`-equivalente |
| Limite do binário | 15-50 MB | **15 MB** (mais apertado) |

A lógica mcpix é **idêntica** nos dois: restaura a Seed uma vez no
init, gera `transport_field` offline por transação.

## A inversão que o mcpix introduz (igual ao iOS)

Spec original (passo 3 "Requisição de Carga"):
```
Instant App → backend → API Pix Banco Central → QR    (rede por transação)
```

Com mcpix:
```
Instant App (Seed restaurada no init) → generateCharge() offline → QR
```

| | Spec original | Com mcpix |
|---|---|---|
| Rede por transação | sim | **não** (só no init) |
| Offline após init | não | **sim** |
| Limite 15 MB do Instant App | ZXing + Retrofit/Ktor pesam | mcpix `.so` ~600 KB/ABI; sem stack HTTP por-txn |

> O limite de 15 MB do Google é **mais apertado** que o da Apple.
> Gerar offline ajuda duplamente: além de eliminar a rede
> por-transação, dispensa o cliente HTTP pesado (Retrofit/Ktor +
> OkHttp) que a spec usa só para a chamada de carga — sobra orçamento
> de binário.

## Fluxo completo

```
┌─ Google Wallet (GenericObject, cloud) ─────────────────┐
│  textModulesData: TITULAR, CHAVE DE IDENTIFICAÇÃO       │
│  barcode PDF_417 = identidade                          │
│  linksModuleData: "Receber Pagamento via Pix" (botão)  │
└───────────────────────────┬────────────────────────────┘
                            │ clique no link (App Link autoVerify)
                            ▼
┌─ Instant App (PixCheckoutActivity) ────────────────────┐
│  1. onCreate → intent.data.getQueryParameter("id")     │
│  2. fetch backend: valida chave + resgata Seed selada  │
│     (mcpix-backup blob) + nome do titular              │
│  3. McpixReceiver.fromSealedBackup(blob, passphrase)   │
│  4. usuário digita valor (Jetpack Compose)             │
│  5. generateCharge(seedId, amountCents) → OFFLINE      │
│  6a. renderiza QR                                       │
│  6b. OU envia via NFC (HCE / NDEF) ao encostar no       │
│      terminal do pagador                               │
└────────────────────────────────────────────────────────┘
                            │ pagador lê QR/NFC → transport_field
                            ▼
       banco do pagador: lookup_seed + apply_recover_c2 → liquida
       → (opcional) HTTP PATCH no GenericObject p/ atualizar status
```

## Estrutura de arquivos

```
google-wallet-instant-app/
├── README.md                       ← este arquivo
├── wallet/
│   ├── generic_object.json         ← GenericObject (payload da REST API)
│   └── README.md                   ← JWT "Save to Google Wallet" + PATCH
├── well-known/
│   └── assetlinks.json             ← Digital Asset Links (App Link)
└── instantapp/
    ├── build.gradle.kts            ← módulo Instant App
    └── src/main/
        ├── AndroidManifest.xml     ← intent-filter autoVerify
        ├── res/values/strings.xml
        └── java/.../
            ├── PixCheckoutActivity.kt  ← captura intent.data (spec §4.1)
            ├── PixTerminalScreen.kt    ← Compose: input + QR
            ├── PixGenerator.kt         ← wrapper do mcpix binding
            ├── QrCode.kt               ← QR via CIQRCodeGenerator-equiv
            └── NfcBeam.kt              ← HCE/NDEF: envia transport_field
```

## Setup (resumo)

1. **Google Wallet GenericObject**: criar a Class + Object via REST API
   (`walletobjects.googleapis.com`), assinar o JWT "Save to Google
   Wallet" com a service account GCP. Ver `wallet/README.md`.
2. **Digital Asset Links**: hospedar `assetlinks.json` em
   `https://instantapp.suaempresa.com.br/.well-known/assetlinks.json`
   com o SHA-256 fingerprint do app.
3. **Instant App module**: `com.android.application` com
   `dist:module dist:instant="true"`; intent-filter `autoVerify="true"`.
4. **mcpix AAR**: `cargo xtask build-android && cargo xtask package-aar`
   → dependência do módulo Instant App.

### Limite de 15 MB do Instant App

O `.so` do mcpix-uniffi é ~600 KB por ABI. Para caber folgado no
limite de 15 MB:
- Use `abiFilters` para incluir só `arm64-v8a` (+ `armeabi-v7a` se
  precisar de devices antigos) no módulo instant.
- Não inclua Retrofit/OkHttp se a única chamada de rede é o init
  fetch — `HttpURLResponse` nativo basta.

## NFC: enviar transport_field ao encostar

`NfcBeam.kt` modela o envio via **HCE (Host Card Emulation)** ou
escrita NDEF. O Android tem suporte melhor que o iOS para
peer-to-peer NFC, mas Instant Apps têm restrições de permissão:
- `android.permission.NFC` é concedida sem prompt, mas HCE exige um
  `HostApduService` declarado — que em Instant App pode ter limites.
- Caminho robusto continua sendo o **QR óptico**; NFC é conveniência.

## Segurança (spec §5 + Digital Asset Links)

1. **Adulteração do `?id=`**: igual ao iOS — o `id` é só hint de qual
   conta resgatar. A autorização real é a posse da Seed restaurada do
   backup selado. Trocar o `id` não dá nada ao atacante.
2. **App Link verificado** (`autoVerify` + `assetlinks.json`): garante
   que só o app legítimo (SHA-256 fingerprint registrado) intercepta
   o link — terceiros não conseguem sequestrar o `https://instantapp...`.
3. **JWT do "Save to Wallet"**: assinado com a service account GCP;
   o cartão só é emitido pela sua infra.
4. **Atualização em tempo real**: HTTP PATCH no GenericObject após o
   banco do pagador confirmar liquidação — ortogonal ao mcpix.

## O que o mcpix cobre vs o que fica fora

| Componente | mcpix | Integrador |
|---|---|---|
| Gerar transport_field offline | ✅ `generateCharge` | — |
| Restaurar Seed do backup | ✅ `McpixReceiver.fromSealedBackup` | passphrase / keystore |
| Renderizar QR | — | `QrCode.kt` (ZXing ou nativo) |
| NFC | — | `NfcBeam.kt` + HCE service |
| GenericObject + JWT | — | `wallet/` + service account GCP |
| PATCH status pós-pagamento | — | backend + Wallet REST API |
