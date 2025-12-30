// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "plasma-stream",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "plasma-stream", targets: ["plasma-stream"])
    ],
    targets: [
        .executableTarget(
            name: "plasma-stream",
            linkerSettings: [
                .linkedFramework("CoreSimulator", .when(platforms: [.macOS])),
                .linkedFramework("IOSurface"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("CoreVideo"),
                .linkedFramework("VideoToolbox"),
                .linkedFramework("ImageIO"),
                .unsafeFlags([
                    "-F/Library/Developer/PrivateFrameworks",
                    "-F/Applications/Xcode.app/Contents/Developer/Library/PrivateFrameworks"
                ])
            ]
        )
    ]
)
