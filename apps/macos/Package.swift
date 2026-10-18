// swift-tools-version: 6.0
import PackageDescription
import Foundation

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
let package = Package(
    name: "SmoothMX",
    platforms: [.macOS(.v13)],
    products: [.executable(name: "SmoothMX", targets: ["SmoothMX"])],
    targets: [
        .target(name: "CSmoothMX", publicHeadersPath: "include"),
        .executableTarget(
            name: "SmoothMX", dependencies: ["CSmoothMX"],
            linkerSettings: [
                .unsafeFlags(["-L", root.appendingPathComponent("target/release").path]),
                .linkedLibrary("smoothmx_platform_macos"),
                .linkedFramework("IOKit"), .linkedFramework("CoreFoundation")
            ]
        )
    ]
)
