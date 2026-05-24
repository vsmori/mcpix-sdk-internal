plugins {
    id("com.android.application") version "8.7.0"
    kotlin("android") version "2.0.21"
}

android {
    namespace = "com.mcpix.sample"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.mcpix.sample"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        viewBinding = true
    }
}

dependencies {
    // O .aar do mcpix-sdk vem da source tree via composite build
    // configurado em settings.gradle.kts (root). Em produção:
    //   implementation("br.com.mcpix:mcpix-sdk:0.1.0")
    implementation("br.com.mcpix:mcpix-sdk")

    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.12.0")
    implementation("net.java.dev.jna:jna:5.14.0@aar")
}
