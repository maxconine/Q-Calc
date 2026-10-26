#!/bin/zsh
# The typst feature's cost and speed on this Mac (arm64, release, stripped): the qcalc-ffi library with and
# without `--features typst`, the Swift smoke test linked against each, and the Swift bench
# (native/typst/swift/main.swift) through the generated bindings.
#
#   native/typst/scripts/measure.sh
#
# Output in native/build/typst/.
set -euo pipefail
NATIVE="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$NATIVE/build/typst"
CARGO="${CARGO:-$HOME/.cargo/bin/cargo}"
TARGET=aarch64-apple-darwin
export MACOSX_DEPLOYMENT_TARGET=13.5
SDK="$(xcrun --sdk macosx --show-sdk-path)"
mkdir -p "$OUT"
cd "$NATIVE"

size() { stat -f %z "$1"; }
mb() { printf "%.2f MB" "$(( $1 / 1048576.0 ))"; }

for variant in off on; do
  features=()
  [[ $variant == on ]] && features=(--features typst)
  dir="$OUT/$variant"
  mkdir -p "$dir/swift"
  start=$SECONDS
  "$CARGO" build -q -p qcalc-ffi --release --target $TARGET --lib "${features[@]}"
  echo "typst $variant: release build $(( SECONDS - start )) s (incremental if cached)"
  # the dylib goes in its own folder: next to the .a, `-lqcalc_ffi` would link it instead of the static library
  mkdir -p "$dir/dylib"
  cp "target/$TARGET/release/libqcalc_ffi.a" "$dir/"
  cp "target/$TARGET/release/libqcalc_ffi.dylib" "$dir/dylib/"
  strip -x "$dir/dylib/libqcalc_ffi.dylib" 2>/dev/null
  "$CARGO" run -q -p qcalc-ffi --bin uniffi-bindgen -- generate --library "$dir/dylib/libqcalc_ffi.dylib" \
    --language swift --out-dir "$dir/swift" --no-format >/dev/null
  # the smoke test, linked as the app links it; the generated bindings reference every export, so the whole
  # feature is linked in, as in the app
  xcrun swiftc -O -target arm64-apple-macos13.5 -sdk "$SDK" -module-cache-path "$dir/modules" \
    -Xcc "-fmodule-map-file=$dir/swift/qcalc_ffiFFI.modulemap" -I "$dir/swift" -L "$dir" -lqcalc_ffi \
    -Xlinker -dead_strip \
    "$dir/swift/qcalc_ffi.swift" "$NATIVE/QCalcCore/Sources/QCalcCore/QCalcCore+Async.swift" \
    "$NATIVE/swift-test/main.swift" -o "$dir/swift-test"
  strip -x "$dir/swift-test"
  echo "typst $variant: libqcalc_ffi.a $(mb $(size "$dir/libqcalc_ffi.a")), dylib stripped $(mb $(size "$dir/dylib/libqcalc_ffi.dylib")), linked smoke test stripped $(mb $(size "$dir/swift-test"))"
done

off=$(size "$OUT/off/swift-test"); on=$(size "$OUT/on/swift-test")
echo "smoke test with the feature on but typst unused: +$(mb $(( on - off ))) (dead-stripped)"
offd=$(size "$OUT/off/dylib/libqcalc_ffi.dylib"); ond=$(size "$OUT/on/dylib/libqcalc_ffi.dylib")
echo "dylib cost: +$(mb $(( ond - offd )))"

xcrun swiftc -O -target arm64-apple-macos13.5 -sdk "$SDK" -module-cache-path "$OUT/on/modules" \
  -Xcc "-fmodule-map-file=$OUT/on/swift/qcalc_ffiFFI.modulemap" -I "$OUT/on/swift" -L "$OUT/on" -lqcalc_ffi \
  -Xlinker -dead_strip "$OUT/on/swift/qcalc_ffi.swift" "$NATIVE/typst/swift/main.swift" -o "$OUT/bench"
strip -x "$OUT/bench"
# the bench calls the typst API, so its binary carries what the app would; the smoke test without the feature
# is the baseline (the bench's own Swift is a few KB)
echo "bench (typst used) stripped $(mb $(size "$OUT/bench")): linked cost of typst +$(mb $(( $(size "$OUT/bench") - off )))"
"$OUT/swift-test" >/dev/null 2>&1 || true
"$OUT/on/swift-test" | tail -1
"$OUT/bench" "$NATIVE/typst/tests/corpus.jsonl" "$OUT"
