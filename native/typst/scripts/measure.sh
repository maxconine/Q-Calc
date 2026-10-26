#!/bin/zsh
# The typst feature's cost and speed on this Mac (arm64, release, stripped): the qcalc-ffi library with and
# without `--features typst`, the Swift smoke test linked against each, and the Swift bench
# (native/typst/swift/bench.swift) through the generated bindings.
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
  cp "target/$TARGET/release/libqcalc_ffi.a" "target/$TARGET/release/libqcalc_ffi.dylib" "$dir/"
  strip -x "$dir/libqcalc_ffi.dylib" 2>/dev/null
  "$CARGO" run -q -p qcalc-ffi --bin uniffi-bindgen -- generate --library "$dir/libqcalc_ffi.dylib" \
    --language swift --out-dir "$dir/swift" --no-format >/dev/null
  # the smoke test, linked as the app links it; the generated bindings reference every export, so the whole
  # feature is linked in, as in the app
  xcrun swiftc -O -target arm64-apple-macos13.5 -sdk "$SDK" -module-cache-path "$dir/modules" \
    -Xcc "-fmodule-map-file=$dir/swift/qcalc_ffiFFI.modulemap" -I "$dir/swift" -L "$dir" -lqcalc_ffi \
    -Xlinker -dead_strip \
    "$dir/swift/qcalc_ffi.swift" "$NATIVE/QCalcCore/Sources/QCalcCore/QCalcCore+Async.swift" \
    "$NATIVE/swift-test/main.swift" -o "$dir/swift-test"
  strip -x "$dir/swift-test"
  echo "typst $variant: libqcalc_ffi.a $(mb $(size "$dir/libqcalc_ffi.a")), dylib stripped $(mb $(size "$dir/libqcalc_ffi.dylib")), linked smoke test stripped $(mb $(size "$dir/swift-test"))"
done

off=$(size "$OUT/off/swift-test"); on=$(size "$OUT/on/swift-test")
echo "linked cost of the typst feature: +$(mb $(( on - off ))) per architecture"
offd=$(size "$OUT/off/libqcalc_ffi.dylib"); ond=$(size "$OUT/on/libqcalc_ffi.dylib")
echo "dylib cost: +$(mb $(( ond - offd )))"

xcrun swiftc -O -target arm64-apple-macos13.5 -sdk "$SDK" -module-cache-path "$OUT/on/modules" \
  -Xcc "-fmodule-map-file=$OUT/on/swift/qcalc_ffiFFI.modulemap" -I "$OUT/on/swift" -L "$OUT/on" -lqcalc_ffi \
  "$OUT/on/swift/qcalc_ffi.swift" "$NATIVE/typst/swift/bench.swift" -o "$OUT/bench"
"$OUT/swift-test" >/dev/null 2>&1 || true
"$OUT/on/swift-test" | tail -1
"$OUT/bench" "$NATIVE/typst/tests/corpus.jsonl" "$OUT"
