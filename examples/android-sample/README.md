# Sample Android — Activity consumindo o AAR

Mínimo viável de app Android exercitando a SDK via o `.aar`
publicado por `bindings/kotlin/aar/`. Layout: um botão dispara o
flow, uma TextView mostra a saída.

## Pré-requisitos

- Android Studio (ou linha de comando: `gradle` + `ANDROID_HOME`
  apontando para uma instalação válida do SDK).
- AAR construído via:
  ```bash
  cargo xtask build-android   # gera jniLibs/<abi>/libmcpix_uniffi.so
  cargo xtask package-aar     # bundleia em dist/android/.../mcpix-sdk-*.aar
  ```

  O `settings.gradle.kts` deste sample usa `includeBuild`
  para resolver o módulo `bindings/kotlin/aar` a partir da source
  tree — não precisa publicar em registry Maven local para testar.

## Build & install

Abrir no Android Studio (`File → Open` → `examples/android-sample`)
ou via CLI:

```bash
cd examples/android-sample
gradle :app:assembleDebug
# instala em emulador/device:
adb install app/build/outputs/apk/debug/app-debug.apk
```

## O que a UI faz

1. Cadastra um SeedId (`RECVR1`); Seed gerada via `OsRng` (que no
   Android usa `/dev/urandom`).
2. Gera uma cobrança de R$ 99,00. Exibe `transport_field` (35 chars)
   + counter T.
3. Valida com C₂ propositalmente errado (`AAAAAAAAAAA`) — espera
   `MISMATCH` para demonstrar a defesa anti-tampering.

Em produção, passos adicionais:
- Mostrar o transport_field como **QR Code** (use
  `examples/web-demo/` como referência visual).
- Receber o C₂ correto do **banco do pagador** via HTTP mTLS — fora
  do escopo do binding `mcpix-uniffi`; integradores plugam
  `mcpix-bank-receiver::http_client` ou seu próprio HTTP client.
- Persistência cross-restart via custom `SeedStore` (hoje só
  in-memory — `mcpix-receiver-sdk::sqlite_store` é a referência
  Rust; expor via UniFFI exige scaffolding extra).

## Notas de empacotamento

- O AAR carrega `.so` para 4 ABIs (arm64-v8a, armeabi-v7a,
  x86_64, x86). Tamanho final ~600 KB por ABI.
- Para reduzir APK size, configure `ndk.abiFilters` em
  `defaultConfig` se você só usa um subset (ex. arm64-v8a only
  para devices modernos).
- `minSdk = 24` (Android 7.0); compatível com 100% dos devices
  de 2024 com Play Store.

## Por que JNA vs JNI puro

Mesma razão do `kotlin-jvm-sample`: UniFFI gera o glue Kotlin que
chama o `.so` via JNA. Vantagem: você não escreve `extern "C"`
manualmente. JNA tem custo de cold-start de ~100ms na primeira
chamada (carrega `libjnidispatch.so`), depois é equivalente a JNI
puro em throughput.
