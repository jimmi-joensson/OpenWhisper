// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "bench-fluidaudio",
    platforms: [.macOS(.v14)],
    dependencies: [
        // Same FluidAudio version the shipped Mac SwiftUI app uses. Keep
        // this in sync with apps/macos/Package.resolved if it ever drifts.
        .package(url: "https://github.com/FluidInference/FluidAudio.git", from: "0.5.0"),
    ],
    targets: [
        .executableTarget(
            name: "bench-fluidaudio",
            dependencies: [
                .product(name: "FluidAudio", package: "FluidAudio"),
            ],
            path: "Sources/bench-fluidaudio"
        ),
    ]
)
