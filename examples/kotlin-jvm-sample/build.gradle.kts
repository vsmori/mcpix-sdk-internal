// Demo Kotlin JVM consumindo o binding UniFFI via JNA, sem Android.
// Mostra o fluxo do recebedor — register → generate → validate — através
// da fronteira JNI, como referência para integradores que vão fazer o
// mesmo num app Android (ver examples/android-sample/).

plugins {
    kotlin("jvm") version "2.0.21"
    application
}

repositories {
    mavenCentral()
}

dependencies {
    // Reusa os sources do binding gerado em bindings/kotlin/. Em
    // produção, isto viria como AAR/JAR publicado num registry Maven.
    implementation(files("../../bindings/kotlin/src/main/kotlin"))
    implementation("net.java.dev.jna:jna:5.14.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")
    implementation(kotlin("stdlib"))
}

kotlin {
    jvmToolchain(21)
}

sourceSets {
    main {
        kotlin.srcDirs(
            "src/main/kotlin",
            // Inclui os arquivos gerados do binding como source set
            // do sample. Alternativa: empacotar o binding como JAR e
            // referenciar via `implementation(files("..."))`.
            "../../bindings/kotlin/src/main/kotlin",
        )
    }
}

// JNA precisa achar o libmcpix_uniffi.so. Default: target/debug do
// workspace Rust. CI pode sobrescrever:
//   gradle run -Pcdylib.dir=/abs/path/to/lib_dir
val cdylibDir: String = (project.findProperty("cdylib.dir") as String?)
    ?: rootProject.projectDir.resolve("../../target/debug").absolutePath

application {
    mainClass.set("MainKt")
    applicationDefaultJvmArgs = listOf("-Djna.library.path=$cdylibDir")
}
