// swift-tools-version:6.0
import PackageDescription

let package = Package(
  name: "LalelaShim",
  platforms: [.macOS("27.0")],
  products: [
    .library(name: "LalelaShim", type: .static, targets: ["LalelaShim"])
  ],
  targets: [
    .target(
      name: "LalelaShim",
      publicHeadersPath: "include",
      swiftSettings: [
        .swiftLanguageMode(.v6),
        .unsafeFlags(["-strict-concurrency=complete"]),
      ]
    )
  ]
)
