# Sample iOS — SwiftUI consumindo o XCFramework

App SwiftUI minimalista exercitando o binding Swift do mcpix-sdk
via SwiftPM. UI: um botão dispara o flow, um `ScrollView` mostra a
saída em fonte monospace.

⚠️ **Não validado neste ambiente** (build de iOS exige macOS +
Xcode). O código está alinhado com o `Package.swift` do binding em
`bindings/swift/` e segue a API exposta pelo UniFFI Swift codegen.

## Pré-requisitos

- macOS com Xcode 15+
- `MCPixSDKFFI.xcframework` construído via:
  ```bash
  cargo xtask build-ios            # device + simulator
  cargo xtask package-xcframework  # bundleia em bindings/swift/
  ```

## Build & run

### Opção A: SwiftPM CLI (recomendado para CI / iteração rápida)

```bash
cd examples/ios-sample
swift build --triple arm64-apple-ios15.0
```

### Opção B: Xcode

```bash
cd examples/ios-sample
open Package.swift
# Xcode abre — selecione um simulator e Run.
```

## O que a UI faz

1. Cadastra um SeedId (`RECVR1`); Seed gerada via `OsRng` (em iOS
   usa `SecRandomCopyBytes` por baixo).
2. Gera uma cobrança de R$ 99,00. Exibe `transport_field` (35 chars)
   + counter T.
3. Valida com C₂ propositalmente errado (`AAAAAAAAAAA`) — espera
   `Mismatch` para demonstrar a defesa anti-tampering.

## Próximos passos para integração real

- **Secure Enclave**: a `Seed` deveria nascer dentro do enclave via
  `SecKeyCreateRandomKey` com `kSecAttrTokenIDSecureEnclave`. Ver
  `docs/SECURE_ELEMENT.md` para o pattern.
- **QR Code**: encadear `charge.transportField` num `CIQRCodeGenerator`
  para mostrar como código óptico ao pagador.
- **mTLS para banco do pagador**: usar
  `NSURLSession` com `URLSessionDelegate` configurado para client
  certificate auth. Cliente equivalente em Rust está em
  `mcpix-bank-receiver::http_client`.

## Pitfalls conhecidos

- O `Package.swift` deste sample tem `path: "../../bindings/swift"`
  apontando para o source tree. Para usar como dependência via tag
  Git, publique o repo e troque para `.package(url: "...", from: "...")`.
- iOS deployment target 15+ é restrição do `MCPixSDKFFI.xcframework`
  (decisão do build do CI; pode ser baixada se necessário rebuilding
  com targets adicionais em `cargo xtask build-ios`).
