// Android sample — consome o .aar publicado por bindings/kotlin/aar.
// Em produção integrators usam `implementation("br.com.mcpix:mcpix-sdk:X.Y.Z")`
// a partir de um Maven registry. Aqui o sample inclui o módulo AAR local.

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

rootProject.name = "mcpix-android-sample"

include(":app")
// Inclui o módulo AAR do binding diretamente da source tree.
includeBuild("../../bindings/kotlin/aar") {
    dependencySubstitution {
        substitute(module("br.com.mcpix:mcpix-sdk"))
            .using(project(":"))
    }
}
