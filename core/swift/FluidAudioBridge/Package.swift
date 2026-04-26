// swift-tools-version: 5.9
import PackageDescription

// Static library that exposes a small C-ABI surface around FluidAudio so
// the Rust core can drive Parakeet-on-ANE without going through Swift's
// async/concurrency runtime in user code. See `core/build.rs` for the
// `swift build` invocation that produces the .a files we link.
//
// FluidAudio version pinned to match the shipped Mac SwiftUI app
// (apps/macos/OpenWhisper.xcodeproj Package.resolved → 0.13.6). Drift
// here = drift between recognizer behavior in Tauri vs shipped Mac.
let package = Package(
    name: "FluidAudioBridge",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "FluidAudioBridge", type: .static, targets: ["FluidAudioBridge"]),
    ],
    dependencies: [
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.13.6"),
    ],
    targets: [
        .target(
            name: "FluidAudioBridge",
            dependencies: [
                .product(name: "FluidAudio", package: "FluidAudio"),
            ],
            path: "Sources/FluidAudioBridge"
        ),
    ]
)
