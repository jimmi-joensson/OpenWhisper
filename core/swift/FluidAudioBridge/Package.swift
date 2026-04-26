// swift-tools-version: 5.9
import PackageDescription

// Static library that exposes a small C-ABI surface around FluidAudio so
// the Rust core can drive Parakeet-on-ANE without going through Swift's
// async/concurrency runtime in user code. See `core/build.rs` for the
// `swift build` invocation that produces the .a files we link.
//
// FluidAudio version pinned to 0.13.6, matching the retired SwiftUI shell
// (archive/macos/OpenWhisper.xcodeproj Package.resolved). Bumping here is
// fine — the SwiftUI shell is no longer the source of truth — but treat
// any change as a recognizer-behavior change and rerun the bench harness.
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
