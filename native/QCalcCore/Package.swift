// swift-tools-version:5.9
// Q Calc's engine for the Mac app: the UniFFI-generated Swift over the Rust core (`native/ffi`), linked from a
// universal (arm64 + x86_64) static xcframework. Build the xcframework and regenerate the Swift first with
// `native/scripts/build-mac.sh --no-test`. The API is documented in API.md.

import PackageDescription

let package = Package(
    name: "QCalcCore",
    platforms: [.macOS("13.5")],
    products: [
        .library(name: "QCalcCore", targets: ["QCalcCore"]),
    ],
    targets: [
        // libqcalc_ffi.a plus the C header and module map the generated Swift imports (module qcalc_ffiFFI)
        .binaryTarget(name: "qcalc_ffiFFI", path: "qcalc_ffiFFI.xcframework"),
        .target(
            name: "QCalcCore",
            dependencies: ["qcalc_ffiFFI"],
            path: "Sources/QCalcCore"
        ),
        .testTarget(
            name: "QCalcCoreTests",
            dependencies: ["QCalcCore"],
            path: "Tests/QCalcCoreTests"
        ),
    ]
)
