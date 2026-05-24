# Generic Pass — montagem do `.pkpass`

O `pass.json` define o cartão de identidade na Apple Wallet. O verso
(`backFields.appClipLink`) carrega o Universal Link que abre o App
Clip do terminal Pix.

## Arquivos obrigatórios no pacote `.pkpass`

| Arquivo | Obrigatório | Notas |
|---|---|---|
| `pass.json` | ✅ | este diretório |
| `manifest.json` | ✅ | SHA-1 de cada arquivo (gerado no build) |
| `signature` | ✅ | PKCS#7 detached do manifest (cert Apple) |
| `icon.png` / `icon@2x.png` | ✅ | 29×29 / 58×58 |
| `logo.png` / `logo@2x.png` | recomendado | canto superior esquerdo |
| `thumbnail.png` / `thumbnail@2x.png` | ✅ p/ generic | **300×300px, proporção 1:1** — foto do titular, centralizada à direita |

> A spec destaca: para Generic Pass, o `thumbnail.png` (1:1, ~300px)
> é o que aparece como foto de identificação.

## Build + assinatura

Requer certificado **Pass Type ID** emitido pela Apple Developer
account (não incluído neste repo — é credencial sensível).

```bash
# 1. Computa o manifest (SHA-1 de cada arquivo do pacote)
#    Ferramenta de referência: signpass (Apple) ou um script.
#    Exemplo com openssl + jq (esqueleto):
cd build-dir/   # contém pass.json + imagens

# manifest.json: { "pass.json": "<sha1>", "icon.png": "<sha1>", ... }
for f in *; do
  printf '"%s": "%s"\n' "$f" "$(openssl sha1 -r "$f" | cut -d' ' -f1)"
done

# 2. Assina o manifest com o cert Pass Type ID (PKCS#7 detached)
openssl smime -binary -sign \
  -certfile WWDR.pem \
  -signer passcert.pem \
  -inkey passkey.pem \
  -in manifest.json \
  -out signature \
  -outform DER

# 3. Zipa tudo como .pkpass
zip -r ../identidade.pkpass . -x '.*'
```

Em produção, use a ferramenta oficial `signpass` da Apple ou uma lib
como `node-passbook` / `passkit-generator`. O fluxo manual acima é
só didático.

## Universal Link → App Clip

Para o link do `backFields` abrir o App Clip (e não o Safari), hospede
o **AASA file** em
`https://appclip.suaempresa.com.br/.well-known/apple-app-site-association`
(sem extensão, `Content-Type: application/json`):

```json
{
  "appclips": {
    "apps": [
      "APPLE_TEAM_ID_10_DIGITOS.br.com.suaempresa.identificacao.Clip"
    ],
    "details": [
      {
        "appID": "APPLE_TEAM_ID_10_DIGITOS.br.com.suaempresa.identificacao.Clip",
        "paths": ["/checkout*"]
      }
    ]
  }
}
```

E no target App Clip do Xcode, adicione o Associated Domain:
`appclips:appclip.suaempresa.com.br`.

## Relação com o mcpix

O `barcodes[].message` (`SEC-99281-X|Carlos Eduardo Lima`) é a
**identidade estática** do cartão — não é um instrumento mcpix. O
instrumento Pix dinâmico (`transport_field` de 35 chars) é gerado
**no App Clip** via `generateCharge` e renderizado lá como QR — ver
`../AppClip/`.

A `CHAVE DE IDENTIFICAÇÃO` (`SEC-99281-X`) vai no `?id=` do Universal
Link e serve para o App Clip descobrir **qual conta** resgatar do
backend no init. Não é o `SeedId` do mcpix (cujo alfabeto exclui `-`
e `0`); o backend mapeia `memberId → SeedId` provisionado. Ver
`../AppClip/PixGenerator.swift`.
