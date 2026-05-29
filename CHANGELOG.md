# Changelog

Todas as mudanças notáveis deste projeto são documentadas aqui.

O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/)
e o projeto adere a [Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Unreleased]

### Added
- Samples por plataforma exercitando a face do recebedor (register →
  generate → validate): Android (Activity + AAR), iOS (SwiftUI +
  XCFramework), Apple Wallet + App Clip, Google Wallet + Play Instant,
  .NET console (P/Invoke), Kotlin JVM CLI (JNA) e demo embarcado.
- `McpixReceiver.fromSealedBackup()` exposto via UniFFI (Swift + Kotlin)
  para o fluxo de restauração offline do App Clip / Instant App.
- Workflow `samples-mobile.yml` (Android + iOS + Instant App), agora
  também disparado em PRs que tocam os samples, seus bindings, o xtask
  de build ou o próprio workflow (filtro de `paths`).
- Captura de log de CI como artefato baixável em falha (`ci.yml` e
  `samples-mobile.yml`), via wrapper `.github/scripts/ci-shell.sh`.
- Badges de status das seis workflows no `README.md`.

### Changed
- `release.yml` agora classifica cada run em um de três modos
  (`dry-run` / `pre-release` / `final`) via um `mode-gate` job. Modo
  determina se signing é obrigatório, se um GitHub Release é criado, se
  é marcado como pre-release, e se publica em Maven/NuGet. Dry-run via
  `workflow_dispatch` com `dry_run=true` permite ensaiar todo o pipeline
  (build + sign + SHA256SUMS + provenance) sem publicar.

### Fixed
- `samples-mobile.yml`: build do Android sample e do Instant App
  (repositórios do AGP no `includeBuild` do AAR, plugin Compose
  obrigatório no Kotlin 2.0, `android.useAndroidX`, heap da JVM do
  Gradle no dex-merge) e empacotamento do XCFramework iOS (modulemap
  com nome `module.modulemap`).
- `xtask package-aar`: constrói o build Gradle standalone `aar/` com a
  task `assembleRelease`, em vez de assumir `:aar` como subprojeto.
- Sample .NET: `??` não suportado no `.csproj`, `Compile` duplicado
  (NETSDK1022) e `ImplicitUsings` ausente.
- Actions JS forçadas ao Node 24 (`FORCE_JAVASCRIPT_ACTIONS_TO_NODE24`)
  para silenciar a deprecação do Node 20 nos runners GHA.

[Unreleased]: https://github.com/vsmori/mcpix-sdk-internal/commits/main
