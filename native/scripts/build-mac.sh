#!/bin/zsh
# Builds the Rust core for the Mac app: a static library per architecture plus a universal one, the Swift
# bindings UniFFI generates, the QCalcCore Swift package's xcframework, and the Swift tests, which it then runs.
#
#   native/scripts/build-mac.sh            # both architectures, run both tests (x86_64 under Rosetta) and the package's
#   native/scripts/build-mac.sh --no-test  # libraries, bindings and the xcframework only (what mac/build.sh calls)
#   QCALC_FFI_FEATURES=typst …             # cargo features for qcalc-ffi (mac/build.sh sets typst: the preview)
#
# Output in native/build/: <arch>/libqcalc_ffi.a, universal/libqcalc_ffi.a, swift/{qcalc_ffi.swift,
# qcalc_ffiFFI.h, qcalc_ffiFFI.modulemap}, swift-test-<arch>. In native/QCalcCore/: Sources/QCalcCore/qcalc_ffi.swift
# (committed, so a diff shows any API change) and qcalc_ffiFFI.xcframework (gitignored).
set -euo pipefail
NATIVE="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$NATIVE/build"
PKG="$NATIVE/QCalcCore"
CARGO="${CARGO:-$HOME/.cargo/bin/cargo}"
TEST=1
[[ "${1:-}" == "--no-test" ]] && TEST=0

# the app's own floor (macos/build.sh MIN_MACOS)
export MACOSX_DEPLOYMENT_TARGET=13.5
typeset -A TRIPLE=(arm64 aarch64-apple-darwin x86_64 x86_64-apple-darwin)

cd "$NATIVE"
mkdir -p "$OUT/swift" "$OUT/universal"
for arch in arm64 x86_64; do
  echo "Building qcalc-ffi for $arch…"
  "$CARGO" build -p qcalc-ffi --release --target "${TRIPLE[$arch]}" --lib ${QCALC_FFI_FEATURES:+--features=$QCALC_FFI_FEATURES}
  mkdir -p "$OUT/$arch"
  cp "target/${TRIPLE[$arch]}/release/libqcalc_ffi.a" "$OUT/$arch/"
done
lipo -create "$OUT/arm64/libqcalc_ffi.a" "$OUT/x86_64/libqcalc_ffi.a" -output "$OUT/universal/libqcalc_ffi.a"

echo "Generating Swift bindings…"
"$CARGO" run -q -p qcalc-ffi --bin uniffi-bindgen -- generate \
  --library "target/aarch64-apple-darwin/release/libqcalc_ffi.dylib" \
  --language swift --out-dir "$OUT/swift" --no-format
cp "$OUT/swift/qcalc_ffi.swift" "$PKG/Sources/QCalcCore/qcalc_ffi.swift"

echo "Building qcalc_ffiFFI.xcframework…"
HEADERS="$OUT/xcframework-headers"
rm -rf "$HEADERS" "$PKG/qcalc_ffiFFI.xcframework"
mkdir -p "$HEADERS"
cp "$OUT/swift/qcalc_ffiFFI.h" "$HEADERS/"
# an xcframework's headers carry their module map under this exact name
cp "$OUT/swift/qcalc_ffiFFI.modulemap" "$HEADERS/module.modulemap"
xcodebuild -create-xcframework \
  -library "$OUT/universal/libqcalc_ffi.a" -headers "$HEADERS" \
  -output "$PKG/qcalc_ffiFFI.xcframework" >/dev/null
lipo -archs "$PKG"/qcalc_ffiFFI.xcframework/*/libqcalc_ffi.a

(( TEST )) || exit 0

for arch in arm64 x86_64; do
  echo "Building the Swift test for $arch…"
  xcrun swiftc -O \
    -target "${arch}-apple-macos${MACOSX_DEPLOYMENT_TARGET}" \
    -sdk "$(xcrun --sdk macosx --show-sdk-path)" \
    -module-cache-path "$OUT/modules-$arch" \
    -Xcc "-fmodule-map-file=$OUT/swift/qcalc_ffiFFI.modulemap" -I "$OUT/swift" \
    -L "$OUT/$arch" -lqcalc_ffi \
    "$PKG/Sources/QCalcCore/qcalc_ffi.swift" "$PKG/Sources/QCalcCore/QCalcCore+Async.swift" \
    "$NATIVE/swift-test/main.swift" \
    -o "$OUT/swift-test-$arch"
  lipo -archs "$OUT/swift-test-$arch"
  if [[ "$arch" == "$(uname -m)" ]]; then
    "$OUT/swift-test-$arch"
  elif arch -x86_64 /usr/bin/true 2>/dev/null; then
    arch "-$arch" "$OUT/swift-test-$arch"
  else
    echo "Skipping the $arch run (no Rosetta)."
  fi
done

echo "Testing the QCalcCore package through SwiftPM…"
swift test --package-path "$PKG" --scratch-path "$OUT/spm"
