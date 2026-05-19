rootProject.name = "mcpix-aar"

// AAR build é separado do projeto JVM (../) porque o Android Gradle Plugin
// puxa o Android SDK inteiro. Mantemos isolado para que dev sem Android SDK
// possa rodar os smoke tests JVM sem fricção.
