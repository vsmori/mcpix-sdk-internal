# Google Wallet GenericObject — emissão + atualização

Diferente do `.pkpass` da Apple (arquivo local assinado), a Google
Wallet é **cloud-first**: você cria uma `GenericClass` + `GenericObject`
via REST API, e distribui o cartão via um JWT "Save to Google Wallet".

## 1. Criar a Class (uma vez por tipo de cartão)

```bash
# POST autenticado com a service account GCP (OAuth2 scope
# https://www.googleapis.com/auth/wallet_object.issuer)
curl -X POST \
  https://walletobjects.googleapis.com/walletobjects/v1/genericClass \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{ "id": "3388000000012345678.ID_CORPORATIVO_CLASS" }'
```

## 2. Criar o Object (por usuário/cartão)

`generic_object.json` (neste diretório) é o payload. POST:

```bash
curl -X POST \
  https://walletobjects.googleapis.com/walletobjects/v1/genericObject \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d @generic_object.json
```

## 3. Botão "Save to Google Wallet" (JWT assinado)

A distribuição expõe um JWT assinado com a chave privada da service
account GCP. O `linksModuleData.uris[].uri` carrega o App Link do
Instant App (`https://instantapp.suaempresa.com.br/checkout?id=...`).

```
{
  "iss": "service-account@projeto.iam.gserviceaccount.com",
  "aud": "google",
  "typ": "savetowallet",
  "payload": {
    "genericObjects": [ { "id": "3388000000012345678.SEC_99281_X" } ]
  }
}
```

Assinar com RS256 usando a private key da service account; o botão
`https://pay.google.com/gp/v/save/<JWT>` abre o "Adicionar à Carteira".

## 4. Atualização em tempo real (pós-pagamento)

Quando o banco do pagador confirma a liquidação, o backend dispara
um **HTTP PATCH** no Object para mudar status / texto:

```bash
curl -X PATCH \
  "https://walletobjects.googleapis.com/walletobjects/v1/genericObject/3388000000012345678.SEC_99281_X" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{ "header": { "defaultValue": { "language": "pt-BR", "value": "PAGO" } } }'
```

O Google propaga via push do Google Play Services instantaneamente —
não precisa de APNs como no iOS.

## Relação com o mcpix

O `barcode.value` (`SEC-99281-X|...`) é a **identidade estática** do
cartão — não é instrumento mcpix. O Pix dinâmico (`transport_field`
de 35 chars) é gerado **no Instant App** via `generateCharge` e
renderizado lá como QR. A `CHAVE DE IDENTIFICAÇÃO` vai no `?id=` e
serve para o Instant App descobrir qual conta resgatar do backend —
o backend mapeia `memberId → SeedId` mcpix provisionado.
