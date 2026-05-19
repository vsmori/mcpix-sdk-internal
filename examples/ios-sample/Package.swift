// swift-tools-version:5.9
//
// SwiftPM app standalone consumindo o MCPixSDK como dependência local.
// Para abrir no Xcode: `open Package.swift` em macOS, ou crie um
// Xcode project apontando para esta pasta. iOS deployment target 15+
// (mesmo do binding).

import PackageDescription

let package = Package(
    name: "McpixSample",
    platforms: [.iOS(.v15), .macOS(.v12)],
    products: [
        .executable(name: "McpixSample", targets: ["McpixSample"]),
    ],
    dependencies: [
        // Em produção: .package(url: "https://github.com/.../MCPixSDK", from: "0.1.0")
        // Aqui consumimos o Package.swift da source tree do repo.
        .package(path: "../../bindings/swift"),
    ],
    targets: [
        .executableTarget(
            name: "McpixSample",
            dependencies: [
                .product(name: "MCPixSDK", package: "swift"),
            ],
            path: "McpixSample"
        ),
    ]
)
