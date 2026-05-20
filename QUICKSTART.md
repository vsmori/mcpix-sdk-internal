# QUICKSTART — primeiros 5 minutos

Ponto de partida para integradores. O `README.md` cobre a SDK
inteira em detalhe (400+ linhas); este aqui é o mínimo para você
ver código rodando.

## Caminho mais rápido: Rust host

```bash
git clone https://github.com/vsmori/mcpix-sdk-internal
cd mcpix-sdk-internal
cargo run --example e2e_demo
```

Saída esperada: 10 passos numerados mostrando o protocolo
completo — recebedor gera cobrança, banco do pagador recupera C₂
via lookup, recebedor valida. Inclui modo timestamp quantizado e
demo de drift de relógio.

## Browser (zero setup nativo)

```bash
cargo install wasm-bindgen-cli --version 0.2.121
rustup target add wasm32-unknown-unknown
cargo xtask build-wasm
cd examples/web-demo
python3 -m http.server 8080
# abre http://localhost:8080
```

UI side-by-side: banco recebedor à esquerda, banco do pagador à
direita, log no fundo. Modo sequencial OU quantizado por tempo
(toggle no topo).

## Por plataforma alvo

| Sua stack | Próximo passo |
|---|---|
| Rust (server, CLI) | importa `mcpix-receiver-sdk` direto; veja [`examples/e2e_demo.rs`](examples/e2e_demo.rs) |
| .NET 8 | [`examples/dotnet-sample/`](examples/dotnet-sample/) |
| Kotlin JVM / Android | [`examples/android-sample/`](examples/android-sample/) (ou [`kotlin-jvm-sample/`](examples/kotlin-jvm-sample/) para CLI sem UI) |
| iOS (SwiftUI) | [`examples/ios-sample/`](examples/ios-sample/) |
| Bare-metal (Cortex-M, ESP32, STM32) | [`embedded/`](embedded/) + [`examples/embedded-demo/README.md`](examples/embedded-demo/README.md) |
| Browser (WASM) | [`examples/web-demo/`](examples/web-demo/) |

## Os 3 conceitos centrais

A SDK implementa **substituição institucional offline**. Em uma
frase: o recebedor gera um par `(C₁ público, C₂ secreto)` sem rede;
o pagador remonta o `C₂` esperado a partir da `Seed` que o banco
recebedor custodia.

| Termo | O que é |
|---|---|
| `Seed` | 32 bytes secretos, custodiados pelo banco recebedor e pelo dispositivo. Substituível por chave em Secure Enclave / HSM ([`docs/SECURE_ELEMENT.md`](docs/SECURE_ELEMENT.md)) |
| `C₁` | Código público de 11 chars derivado de `HMAC(Seed, T)`. Vai no QR/NFC/SMS |
| `C₂` | Código secreto de 11 chars derivado de `HMAC(Seed, T, C₁)`. Confirmação da transação |
| `T` | Counter (sequencial ou timestamp quantizado em janela de 30s) |

Detalhe completo do protocolo em [`docs/PROTOCOL.md`](docs/PROTOCOL.md).

## Próximos documentos por área

| Quero entender... | Veja |
|---|---|
| Threat model + mitigações | [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) |
| Por que cada decisão de design | [`docs/adr/`](docs/adr/) (12 ADRs numeradas) |
| Como o protocolo evolui (V1 → V2) | [`docs/VERSIONING.md`](docs/VERSIONING.md) |
| Backup criptografado de sementes | `mcpix-backup` crate |
| mTLS + CRL + OCSP entre bancos | [`docs/MTLS_REVOCATION.md`](docs/MTLS_REVOCATION.md) |
| Reproducibilidade do build | [`docs/SLSA.md`](docs/SLSA.md) e [`docs/SLSA_L4_PROGRESS.md`](docs/SLSA_L4_PROGRESS.md) |
