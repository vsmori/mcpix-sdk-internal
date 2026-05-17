// swift-tools-version:5.9
//
// Swift Package skeleton para o SDK. O artefato `MCPixSDKFFI.xcframework`
// referenciado abaixo será produzido pelo pipeline de CI (próxima sessão)
// compilando `mcpix-uniffi` para `aarch64-apple-ios` + `aarch64-apple-ios-sim`
// + `x86_64-apple-ios-sim` e empacotando os `.a` em XCFramework.
//
// Por enquanto este `Package.swift` documenta a topologia esperada — não
// resolve até o XCFramework existir.

import PackageDescription

let package = Package(
    name: "MCPixSDK",
    platforms: [
        .iOS(.v15),
        .macOS(.v12),
    ],
    products: [
        .library(name: "MCPixSDK", targets: ["MCPixSDK"]),
    ],
    targets: [
        // O XCFramework binário é gerado pelo pipeline. Em dev local pode
        // ser apontado para um path relativo após `cargo xtask build-ios`
        // (a ser adicionado na sessão 3).
        .binaryTarget(
            name: "MCPixSDKFFI",
            path: "MCPixSDKFFI.xcframework"
        ),
        .target(
            name: "MCPixSDK",
            dependencies: ["MCPixSDKFFI"],
            path: "Sources/MCPixSDK"
        ),
    ]
)
