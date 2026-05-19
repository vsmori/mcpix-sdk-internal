# Mantém classes geradas pelo UniFFI — JNA usa reflexão sobre os tipos
# `Pointer`, `Structure` e callbacks. Sem essas regras o R8 stripa símbolos
# que o cdylib resolve por nome em runtime.
-keep class com.sun.jna.** { *; }
-keep class uniffi.mcpix_uniffi.** { *; }
-keepclassmembers class uniffi.mcpix_uniffi.** { *; }
