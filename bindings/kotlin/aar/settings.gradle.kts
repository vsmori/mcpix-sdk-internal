rootProject.name = "mcpix-aar"

// AAR build é separado do projeto JVM (../) porque o Android Gradle Plugin
// puxa o Android SDK inteiro. Mantemos isolado para que dev sem Android SDK
// possa rodar os smoke tests JVM sem fricção.
//
// Este build é incluído via `includeBuild` pelos samples (android-sample,
// google-wallet-instant-app). Builds incluídos NÃO herdam o pluginManagement
// nem os repositórios do build pai — cada um resolve plugins/deps pelo seu
// próprio settings. Por isso os repositórios precisam estar declarados aqui:
// o Android Gradle Plugin (`com.android.library`) vem do repositório google().

pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}
