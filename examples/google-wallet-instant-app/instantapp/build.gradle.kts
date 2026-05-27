// Módulo Instant App. Para Google Play Instant moderno, o app é
// distribuído como App Bundle com um módulo marcado `instant=true`.
// Aqui modelamos o módulo de checkout isolado — a spec §5 enfatiza
// manter ESTE módulo livre de deps pesadas (limite de 15 MB).

plugins {
    id("com.android.application") version "8.7.0"
    kotlin("android") version "2.0.21"
    // Desde Kotlin 2.0 o compilador Compose virou um plugin Gradle separado
    // e obrigatório quando `buildFeatures.compose = true`. A versão acompanha
    // a do Kotlin.
    id("org.jetbrains.kotlin.plugin.compose") version "2.0.21"
}

android {
    namespace = "br.com.suaempresa.identificacao"
    compileSdk = 34

    defaultConfig {
        applicationId = "br.com.suaempresa.identificacao"
        minSdk = 24 // Instant Apps: minSdk 21+; 24 cobre o parque atual
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"

        // Limite de 15 MB: restringe ABIs ao essencial. arm64-v8a
        // cobre devices modernos; adicione armeabi-v7a só se precisar
        // de aparelhos antigos.
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }

    buildFeatures {
        compose = true
    }
    // `composeOptions.kotlinCompilerExtensionVersion` não é mais usado com o
    // plugin org.jetbrains.kotlin.plugin.compose (Kotlin 2.0+) — o plugin
    // fixa a versão do compilador junto com a do Kotlin.
}

dependencies {
    // mcpix AAR (cargo xtask package-aar). Em produção:
    //   implementation("br.com.mcpix:mcpix-sdk:0.1.0")
    implementation("br.com.mcpix:mcpix-sdk")

    // Instant App distribution module
    implementation("com.google.android.gms:play-services-instantapps:18.1.0")

    // Compose mínimo — a spec §5 pede evitar deps pesadas. Sem
    // Retrofit/OkHttp: o único fetch (init) usa HttpURLConnection.
    implementation(platform("androidx.compose:compose-bom:2024.09.03"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.activity:activity-compose:1.9.2")

    // QR rendering. ZXing core é leve (~100 KB) — cabe no orçamento.
    implementation("com.google.zxing:core:3.5.3")

    // JNA para o binding mcpix carregar o .so (vem com o AAR).
    implementation("net.java.dev.jna:jna:5.14.0@aar")
}
