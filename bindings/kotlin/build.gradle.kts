// Smoke test do binding Kotlin gerado por UniFFI.
//
// Carrega o cdylib produzido pelo workspace Rust (libmcpix_uniffi.so) e
// exercita register → generate_charge → validate_receipt através do binding
// idiomático. Não é um build de produção — é apenas prova-de-vida de que o
// binding gerado funciona com o cdylib.

plugins {
    kotlin("jvm") version "2.0.21"
    application
}

repositories {
    mavenCentral()
}

dependencies {
    // UniFFI Kotlin runtime: JNA para chamar a biblioteca nativa.
    implementation("net.java.dev.jna:jna:5.14.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")
    implementation(kotlin("stdlib"))
    testImplementation(kotlin("test"))
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.0")
}

kotlin {
    jvmToolchain(21)
}

sourceSets {
    main {
        kotlin.srcDirs("src/main/kotlin")
    }
    test {
        kotlin.srcDirs("src/test/kotlin")
    }
}

// Encaminha o caminho do cdylib via java.library.path. JNA também aceita
// `jna.library.path`. Apontamos para `../../target/debug` (saída padrão de
// `cargo build`). Pipeline CI pode sobrescrever via -Pcdylib.dir=...
val cdylibDir: String = (project.findProperty("cdylib.dir") as String?)
    ?: rootProject.projectDir.resolve("../../target/debug").absolutePath

tasks.withType<Test> {
    useJUnitPlatform()
    systemProperty("jna.library.path", cdylibDir)
    systemProperty("java.library.path", cdylibDir)
    testLogging {
        events("passed", "failed", "skipped")
        showStandardStreams = true
    }
}

application {
    mainClass.set("MainKt")
    applicationDefaultJvmArgs = listOf("-Djna.library.path=$cdylibDir")
}
