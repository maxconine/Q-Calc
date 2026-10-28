#!/bin/zsh
# Builds the native Q Calc (no web view for the calculator): the Rust core as a static library per architecture,
# the UniFFI Swift bindings as their own module (QCalcFFI), the app for arm64 and x86_64, lipo'd into one
# universal binary, and the bundle with Sparkle and (if built) the games bundle, the app's only web code.
#
#   zsh native/mac/build.sh              # dev build: native/mac/dist/Q Calc Dev.app (com.maxconine.qcalc.native-dev)
#   zsh native/mac/build.sh --run        # …and launch it (quitting an earlier dev build first, never the installed app)
#   zsh native/mac/build.sh --zip        # …and a zip to hand testers (native/mac/dist/Q-Calc-Dev-<version>-<commit>.zip)
#   zsh native/mac/build.sh --arch=arm64 # one architecture only (faster while iterating)
#   zsh native/mac/build.sh --skip-rust  # reuse native/build's libraries and bindings
#   zsh native/mac/build.sh --release    # Q Calc's own bundle id, name, hotkey and Sparkle key (not for side-by-side use)
#
# Inputs it finds by itself, or from the environment:
#   QCALC_SPARKLE_DIR  a Sparkle-2.10.0 release folder (default macos/vendor/Sparkle-2.10.0, fetched like macos/build.sh)
#   QCALC_GAMES_WEB    a built games bundle (games.html and its assets); by default it is built into
#                      native/build/games-web from games.vite.config.ts when node_modules is there; without one
#                      games and the 420/67/69 effects are off
set -euo pipefail
export COPYFILE_DISABLE=1

MAC="$(cd "$(dirname "$0")" && pwd)"
NATIVE="$(cd "$MAC/.." && pwd)"
ROOT="$(cd "$NATIVE/.." && pwd)"
OUT="$NATIVE/build"
MIN_MACOS="12.0"
SPARKLE_VERSION="2.10.0"
SPARKLE_SHA256="c2bf58aa8387266ac179357b1415d6f2635f044da8be41042af32425dae6da0c"
SPARKLE="${QCALC_SPARKLE_DIR:-$ROOT/macos/vendor/Sparkle-${SPARKLE_VERSION}}"
GAMES_WEB="${QCALC_GAMES_WEB:-$OUT/games-web}"
SDK="$(xcrun --sdk macosx --show-sdk-path)"
RUN=0
ZIP=0
SKIP_RUST=0
RELEASE=0
ARCHS=(arm64 x86_64)

for arg in "$@"; do
  case "$arg" in
    --run) RUN=1 ;;
    --zip) ZIP=1 ;;
    --skip-rust) SKIP_RUST=1 ;;
    --release) RELEASE=1 ;;
    --arch=*) ARCHS=("${arg#--arch=}") ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    *) echo "Unknown option: $arg" >&2; exit 1 ;;
  esac
done

if (( RELEASE )); then
  APP_NAME="Q Calc"
else
  APP_NAME="Q Calc Dev"
fi
DIST_APP="$MAC/dist/$APP_NAME.app"
APP_VERSION="$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$ROOT/package.json" | head -1)"

fetch_sparkle() {
  [[ -d "$SPARKLE/Sparkle.framework" ]] && return
  mkdir -p "$(dirname "$SPARKLE")"
  local archive="$SPARKLE.tar.xz"
  echo "Downloading Sparkle ${SPARKLE_VERSION}…"
  curl -L --fail -o "$archive" \
    "https://github.com/sparkle-project/Sparkle/releases/download/${SPARKLE_VERSION}/Sparkle-${SPARKLE_VERSION}.tar.xz"
  [[ "$(shasum -a 256 "$archive" | awk '{print $1}')" == "$SPARKLE_SHA256" ]] || { echo "Sparkle checksum mismatch" >&2; exit 1; }
  mkdir -p "$SPARKLE"
  tar -xJf "$archive" -C "$SPARKLE"
}

# 1. the engine
if (( ! SKIP_RUST )); then
  # typst: the typeset preview (TypstRenderer in the generated Swift)
  QCALC_FFI_FEATURES="${QCALC_FFI_FEATURES:-typst}" zsh "$NATIVE/scripts/build-mac.sh" --no-test
fi
[[ -d "$NATIVE/QCalcCore/qcalc_ffiFFI.xcframework" ]] || { echo "No QCalcCore xcframework; run without --skip-rust" >&2; exit 1; }
fetch_sparkle
export QCALC_SPARKLE_DIR="$SPARKLE"
# the games bundle: games.html alone (none of the calculator's pages), when the repo's node_modules are there
if [[ -z "${QCALC_GAMES_WEB:-}" && -x "$ROOT/node_modules/.bin/vite" ]]; then
  echo "Building the games bundle…"
  (cd "$ROOT" && QCALC_GAMES_OUT="$GAMES_WEB" node_modules/.bin/vite build --config native/mac/games.vite.config.ts >/dev/null)
fi
# Sources/Shared: symlinks to the macos/ files compiled unchanged
zsh "$MAC/scripts/link-shared.sh"

STAGE="$(mktemp -d /tmp/qcalc-native.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT
APP="$STAGE/$APP_NAME.app"
BIN="$APP/Contents/MacOS/QCalc"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"

# 2. the app through SwiftPM, against the QCalcCore package (both architectures in one universal binary)
# one native build per architecture, lipo'd: SwiftPM's multi-arch (xcbuild) path links the static xcframework
# as a dynamic product and can't find it
THIN=()
for arch in "${ARCHS[@]}"; do
  echo "Building Q Calc for $arch…"
  SPM_ARGS=(--package-path "$MAC" -c release --scratch-path "$OUT/mac-spm-$arch" --triple "${arch}-apple-macosx${MIN_MACOS}")
  swift build "${SPM_ARGS[@]}"
  THIN+=("$(swift build "${SPM_ARGS[@]}" --show-bin-path)/QCalcMac")
done
lipo -create "${THIN[@]}" -output "$BIN"
# every symbol: nothing looks the executable's own up by name (the SwiftPM build's dSYM keeps them for crash logs)
strip "$BIN"
chmod +x "$BIN"

# 4. the bundle
PLIST="$APP/Contents/Info.plist"
cp "$MAC/Info.plist" "$PLIST"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $APP_VERSION" "$PLIST"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $APP_VERSION" "$PLIST"
if (( RELEASE )); then
  # the installed app's identity, feed and key: this build replaces macos/'s, so never run it beside one
  /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier com.maxconine.qcalc" "$PLIST"
  /usr/libexec/PlistBuddy -c "Set :CFBundleName Q Calc" -c "Set :CFBundleDisplayName Q Calc" "$PLIST"
  /usr/libexec/PlistBuddy -c "Delete :QCDevBuild" -c "Set :QCSupportFolder Q Calc" -c "Delete :QCDefaultHotKey" "$PLIST"
  key="$(/usr/libexec/PlistBuddy -c "Print :SUPublicEDKey" "$ROOT/macos/Info.plist")"
  /usr/libexec/PlistBuddy -c "Set :SUPublicEDKey $key" "$PLIST"
  BUNDLE_ID=com.maxconine.qcalc
else
  # a dev build has the appcast but no key, so Updates.start() never starts Sparkle
  BUNDLE_ID=com.maxconine.qcalc.native-dev
fi

ditto "$SPARKLE/Sparkle.framework" "$APP/Contents/Frameworks/Sparkle.framework"
rm -rf "$APP/Contents/Frameworks/Sparkle.framework/XPCServices" "$APP/Contents/Frameworks/Sparkle.framework/Versions/B/XPCServices"

icon_src="$ROOT/public/Qcalc_favi.png"
if [[ -f "$icon_src" ]]; then
  # the menu bar icon at the 18 pt it's shown, 1x and 2x (drawn from the full PNG, it kept a 6 MB decoded copy)
  sips -s format png -z 18 18 "$icon_src" --out "$APP/Contents/Resources/StatusIcon.png" >/dev/null
  sips -s format png -z 36 36 "$icon_src" --out "$APP/Contents/Resources/StatusIcon@2x.png" >/dev/null
  iconset="$STAGE/AppIcon.iconset"
  mkdir -p "$iconset"
  sips -s format png -z 1024 1024 "$icon_src" --out "$STAGE/icon.png" >/dev/null
  for size in 16 32 128 256 512; do
    sips -z $size $size "$STAGE/icon.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z $((size * 2)) $((size * 2)) "$STAGE/icon.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
  done
  iconutil -c icns "$iconset" -o "$APP/Contents/Resources/AppIcon.icns"
fi

# the games bundle: games.html and the assets it loads, nothing of the calculator's pages
if [[ -f "$GAMES_WEB/games.html" ]]; then
  ditto "$GAMES_WEB" "$APP/Contents/Resources/web"
  echo "Bundled the games from $GAMES_WEB"
else
  echo "warning: no $GAMES_WEB/games.html (needs node_modules); games and the 420/67/69 effects are off in this build" >&2
fi

# 5. sign, inside out, ad hoc (the release workflow signs for real)
xattr -cr "$APP"
sparkle="$APP/Contents/Frameworks/Sparkle.framework"
codesign --force --sign - --timestamp=none -o runtime "$sparkle/Versions/B/Autoupdate"
codesign --force --sign - --timestamp=none -o runtime "$sparkle/Versions/B/Updater.app"
codesign --force --sign - --timestamp=none -o runtime "$sparkle/Versions/B"
codesign --force --sign - --timestamp=none -o runtime "$sparkle"
codesign --force --sign - --timestamp=none --identifier "$BUNDLE_ID" "$APP"
codesign --verify --deep --strict "$APP"

if (( RUN )); then
  # only this build's own process; the installed Q Calc has another bundle id and is never touched
  pkill -f "$DIST_APP/Contents/MacOS/QCalc" 2>/dev/null || true
fi
rm -rf "$DIST_APP"
mkdir -p "$MAC/dist"
ditto "$APP" "$DIST_APP"
echo "Built $DIST_APP ($APP_VERSION, $(lipo -archs "$DIST_APP/Contents/MacOS/QCalc"), $(du -sh "$DIST_APP" | cut -f1))"
if (( ZIP )); then
  # zipped from the stage, without Finder metadata, so the signature survives the trip to a tester
  ZIP_PATH="$MAC/dist/$(echo "$APP_NAME" | tr ' ' '-')-$APP_VERSION-$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo local).zip"
  rm -f "$ZIP_PATH"
  ditto -c -k --norsrc --keepParent "$APP" "$ZIP_PATH"
  echo "Packaged $ZIP_PATH ($(du -h "$ZIP_PATH" | cut -f1))"
fi
if (( RUN )); then
  open "$DIST_APP" --args --show
fi
