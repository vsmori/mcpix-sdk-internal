# Examples — pontos de partida por plataforma

Cada sub-pasta é um app/projeto minimalista exercitando a SDK na
plataforma respectiva. Todos seguem **o mesmo fluxo conceitual** do
recebedor — register → generate → validate — para serem comparáveis
quando você porta para outro stack.

## Mapa

| Pasta | Stack | Status | Validado neste env |
|---|---|---|---|
| [`e2e_demo.rs`](e2e_demo.rs) | Rust host (CLI) | ✅ produção | `cargo run --example e2e_demo` |
| [`web-demo/`](web-demo/) | WASM + HTML/JS browser | ✅ produção | `cargo xtask build-wasm` + http server |
| [`dotnet-sample/`](dotnet-sample/) | .NET 8 console (P/Invoke) | ✅ código completo | ❌ dotnet não disponível aqui |
| [`kotlin-jvm-sample/`](kotlin-jvm-sample/) | Kotlin JVM CLI (JNA) | ✅ código completo | 🟡 build Gradle exige toolchain |
| [`android-sample/`](android-sample/) | Android Activity + AAR | ✅ código completo | 🟡 build exige Android SDK |
| [`ios-sample/`](ios-sample/) | iOS SwiftUI + XCFramework | ⚠️ skeleton | ❌ exige macOS + Xcode |
| [`embedded-demo/`](embedded-demo/) | Cortex-M4F bare-metal (`no_std`) | ✅ produção | ✅ cross-compile thumbv7em |

> **Por que não E2E completo em cada sample**: a face exposta pelos
> bindings (`mcpix-uniffi`) é só a do **recebedor** — register,
> generate, validate. O `C₂` correto chega em produção via banco do
> pagador (HTTP mTLS); esse caminho está em `mcpix-bank-receiver` e
> não atravessa o UniFFI. Os samples portanto exercitam **a face
> do integrador** (que é o que importa para quem está escrevendo um
> app real) e mostram `Mismatch` na validação com um C₂ inválido
> para demonstrar a defesa anti-tampering.
>
> Para o fluxo conceitual completo (recebedor + pagador num único
> processo), veja [`e2e_demo.rs`](e2e_demo.rs) ou
> [`web-demo/`](web-demo/).

## Convenção comum

Todos os samples (exceto `embedded-demo/`) exercitam o mesmo cenário:

1. `register("RECVR1")` — gera Seed local de 32 bytes.
2. `generateCharge("RECVR1", 9900)` — cobrança de R$ 99,00.
3. Print `transport_field` (35 chars) + `counter`.
4. `validateReceipt("RECVR1", counter, "AAAAAAAAAAA")` — C₂
   deliberadamente errado para mostrar `Mismatch`.

Cada sample tem `README.md` próprio com build instructions
específicas.

## Próximos passos pelo integrador

Após rodar o sample, três caminhos típicos:

| Caminho | Onde olhar |
|---|---|
| Persistência (custom `SeedStore`) | `crates/mcpix-receiver-sdk::sqlite_store` (Rust); para mobile, expor via UniFFI callback interface |
| Custódia em Secure Element | `docs/SECURE_ELEMENT.md` |
| Backup criptografado de sementes | `crates/mcpix-backup` |
| Banco do pagador (mTLS) | `crates/mcpix-bank-receiver` |
| QR Code visual | `crates/mcpix-embed::qr` (embed) ou `qrcodegen` JS (browser) |
