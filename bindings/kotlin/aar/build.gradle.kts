// AAR package contendo:
// - jniLibs/<abi>/libmcpix_uniffi.so  (compilado por `cargo xtask build-android`)
// - .kt gerado por UniFFI              (reaproveitado de ../src/main/kotlin/)
//
// Requer Android SDK (ANDROID_HOME apontando para uma instalação válida)
// e Android Gradle Plugin. Sem isso a configuração não resolve — é
// intencional: pipelines sem Android SDK pulam este módulo.

plugins {
    id("com.android.library") version "8.7.0"
    kotlin("android") version "2.0.21"
    `maven-publish`
}

group = "br.com.mcpix"
version = providers.gradleProperty("mcpix.version").getOrElse("0.1.0-SNAPSHOT")

android {
    namespace = "br.com.mcpix.sdk"
    compileSdk = 34

    defaultConfig {
        minSdk = 24
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets {
        named("main") {
            // jniLibs vem de fora — `cargo xtask build-android` deposita aqui
            // (override via -PjniLibs.dir=…). Default aponta para dist/android/jniLibs.
            val jniDir = providers.gradleProperty("jnilibs.dir")
                .getOrElse(rootProject.projectDir.resolve("../../../dist/android/jniLibs").absolutePath)
            jniLibs.srcDir(jniDir)
            // Reaproveita o .kt gerado por UniFFI (mesmo arquivo do smoke test).
            kotlin.srcDir("../src/main/kotlin")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
}

dependencies {
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")
}

publishing {
    publications {
        register<MavenPublication>("release") {
            groupId = project.group.toString()
            artifactId = "mcpix-sdk-android"
            afterEvaluate {
                from(components["release"])
            }
        }
    }
    repositories {
        // URL configurável via -Pmaven.url=… para apontar para o registry interno.
        val mavenUrl = providers.gradleProperty("maven.url").getOrElse("")
        if (mavenUrl.isNotEmpty()) {
            maven {
                url = uri(mavenUrl)
                credentials {
                    username = providers.gradleProperty("maven.user").getOrElse("")
                    password = providers.gradleProperty("maven.pass").getOrElse("")
                }
            }
        } else {
            mavenLocal()
        }
    }
}

kotlin {
    jvmToolchain(17)
}
