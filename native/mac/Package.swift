// swift-tools-version:5.9
// The native Q Calc for Mac: an AppKit/SwiftUI app over the QCalcCore engine package, with no web view for the
// calculator. `build.sh` builds the Rust core and QCalcCore's xcframework, runs `swift build` for arm64 and x86_64,
// and bundles the result (Info.plist, Sparkle, sheet data, the games bundle) into Q Calc Dev.app.
//
// Sources/Shared holds symlinks to the AppKit code the web-view app in macos/ already has (settings, keybinds,
// messages, rates, the periodic table and identity sheets, Sparkle, the login item), compiled unchanged.

import Foundation
import PackageDescription

let here = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
// Sparkle isn't a SwiftPM package in this repo; it's the release folder macos/build.sh fetches
let sparkle = ProcessInfo.processInfo.environment["QCALC_SPARKLE_DIR"]
    ?? here.appendingPathComponent("../../macos/vendor/Sparkle-2.10.0").standardized.path

let package = Package(
    name: "QCalcMac",
    platforms: [.macOS("13.5")],
    dependencies: [
        .package(path: "../QCalcCore"),
    ],
    targets: [
        .executableTarget(
            name: "QCalcMac",
            dependencies: [.product(name: "QCalcCore", package: "QCalcCore")],
            path: "Sources",
            swiftSettings: [.unsafeFlags(["-F", sparkle])],
            linkerSettings: [
                .unsafeFlags(["-F", sparkle, "-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks", "-Xlinker", "-dead_strip"]),
                .linkedFramework("Sparkle"),
                .linkedFramework("SwiftUI"),
                .linkedFramework("AppKit"),
                .linkedFramework("WebKit"),
                .linkedFramework("Carbon"),
                .linkedFramework("ServiceManagement"),
                .linkedFramework("UserNotifications"),
            ]
        ),
    ]
)
