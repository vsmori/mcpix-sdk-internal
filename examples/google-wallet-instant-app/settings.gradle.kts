// Resolve o módulo Instant App + o AAR do mcpix a partir da source
// tree (composite build), espelhando examples/android-sample. Em
// produção: `implementation("br.com.mcpix:mcpix-sdk:X.Y.Z")` de um
// Maven registry.

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "mcpix-google-wallet-instant-app"

include(":instantapp")

// Inclui o módulo AAR do binding direto da source tree e substitui a
// coordenada Maven `br.com.mcpix:mcpix-sdk` por ele.
includeBuild("../../bindings/kotlin/aar") {
    dependencySubstitution {
        substitute(module("br.com.mcpix:mcpix-sdk"))
            .using(project(":"))
    }
}
