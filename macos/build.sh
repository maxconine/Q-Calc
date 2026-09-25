#!/bin/zsh
set -euo pipefail
export COPYFILE_DISABLE=1

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MAC="$ROOT/macos"
DIST_APP="$MAC/dist/Q Calc.app"
VENDOR="$MAC/vendor"
SOULVER_VERSION="3.5.1"
SOULVER_SHA256="36e51abc2d22b2f1000ceeb4468cfa9ce74f4cf3fb79097f7f3e004b7bf9e7c7"
XC="$VENDOR/SoulverCore.xcframework"
ZIP="$VENDOR/SoulverCore.xcframework.zip"
SLICE="$XC/macos-arm64_x86_64"
SPARKLE_VERSION="2.10.0"
SPARKLE_SHA256="c2bf58aa8387266ac179357b1415d6f2635f044da8be41042af32425dae6da0c"
SPARKLE="$VENDOR/Sparkle-${SPARKLE_VERSION}"
SPARKLE_PLACEHOLDER_KEY="SPARKLE_PUBLIC_KEY_PLACEHOLDER"
INSTALL=0
PACKAGE=0
APP_VERSION="$(node -p "require('$ROOT/package.json').version")"
DIST_ZIP="$MAC/dist/Q-Calc-${APP_VERSION}.zip"

for arg in "$@"; do
  case "$arg" in
    --install) INSTALL=1 ;;
    --package) PACKAGE=1 ;;
    -h|--help)
      echo "Usage: macos/build.sh [--install] [--package]"
      echo "  --install   copy Q Calc.app into /Applications"
      echo "  --package   write macos/dist/Q-Calc-<version>.zip for release (needs the real SUPublicEDKey)"
      echo "  QCALC_SPARKLE_PUBLIC_KEY=<key> overrides SUPublicEDKey, for update tests"
      exit 0
      ;;
    *)
      echo "Unknown option: $arg" >&2
      echo "Usage: macos/build.sh [--install] [--package]" >&2
      exit 1
      ;;
  esac
done

# Assemble and sign under /tmp. Codesign rejects Documents-folder provenance xattrs.
STAGE="$(mktemp -d /tmp/qcalc.XXXXXX)"
APP="$STAGE/Q Calc.app"
BIN="$APP/Contents/MacOS/QCalc"
trap 'rm -rf "$STAGE"' EXIT

require() {
  if ! command -v "$1" >/dev/null; then
    echo "$2" >&2
    exit 1
  fi
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "The Mac app can only be built on macOS." >&2
  exit 1
fi

ARCH="$(uname -m)"
if [[ "$ARCH" != "arm64" ]]; then
  echo "Q Calc is documented for Apple silicon. Detected $ARCH; continuing anyway." >&2
fi

require xcrun "Install Xcode 26 or later, then run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer"
require swiftc "Install Xcode 26 or later from the Mac App Store."
require npm "Install Node.js 20 or later from https://nodejs.org or: brew install node"

fetch_soulver() {
  mkdir -p "$VENDOR"
  if [[ -d "$XC" ]]; then
    return
  fi
  echo "Downloading SoulverCore ${SOULVER_VERSION}…"
  curl -L --fail -o "$ZIP" \
    "https://github.com/soulverteam/SoulverCore/releases/download/${SOULVER_VERSION}/SoulverCore.xcframework.zip"
  local got
  got="$(shasum -a 256 "$ZIP" | awk '{print $1}')"
  if [[ "$got" != "$SOULVER_SHA256" ]]; then
    echo "SoulverCore checksum mismatch: $got" >&2
    exit 1
  fi
  unzip -q -o "$ZIP" -d "$VENDOR"
}

# the full release, not just the framework: the workflow signs the zip with bin/sign_update
fetch_sparkle() {
  mkdir -p "$VENDOR"
  if [[ -d "$SPARKLE/Sparkle.framework" ]]; then
    return
  fi
  local archive="$VENDOR/Sparkle-${SPARKLE_VERSION}.tar.xz"
  echo "Downloading Sparkle ${SPARKLE_VERSION}…"
  curl -L --fail -o "$archive" \
    "https://github.com/sparkle-project/Sparkle/releases/download/${SPARKLE_VERSION}/Sparkle-${SPARKLE_VERSION}.tar.xz"
  local got
  got="$(shasum -a 256 "$archive" | awk '{print $1}')"
  if [[ "$got" != "$SPARKLE_SHA256" ]]; then
    echo "Sparkle checksum mismatch: $got" >&2
    exit 1
  fi
  rm -rf "$SPARKLE"
  mkdir -p "$SPARKLE"
  tar -xJf "$archive" -C "$SPARKLE"
}

# a real key is 32 bytes of base64. A release zip with the placeholder could never be updated
set_update_key() {
  local plist="$APP/Contents/Info.plist"
  if [[ -n "${QCALC_SPARKLE_PUBLIC_KEY:-}" ]]; then
    /usr/libexec/PlistBuddy -c "Set :SUPublicEDKey $QCALC_SPARKLE_PUBLIC_KEY" "$plist"
  fi
  local key
  key="$(/usr/libexec/PlistBuddy -c "Print :SUPublicEDKey" "$plist")"
  if [[ "$key" == "$SPARKLE_PLACEHOLDER_KEY" ]]; then
    if (( PACKAGE )); then
      echo "error: SUPublicEDKey in macos/Info.plist is still the placeholder." >&2
      echo "A release built like this can never update itself. See 'Automatic updates' in the README." >&2
      exit 1
    fi
    echo "SUPublicEDKey is the placeholder; this build won't check for updates." >&2
    return
  fi
  if [[ "$(printf %s "$key" | base64 -D 2>/dev/null | wc -c | tr -d ' ')" != "32" ]]; then
    echo "error: SUPublicEDKey is not a base64 EdDSA public key: $key" >&2
    exit 1
  fi
}

make_icon() {
  local resources="$APP/Contents/Resources"
  local work="$STAGE/iconwork"
  local iconset="$work/AppIcon.iconset"
  local src="$ROOT/public/Qcalc_favi.png"
  if [[ ! -f "$src" ]]; then
    echo "Skipping app icon (missing $src)." >&2
    return
  fi
  mkdir -p "$iconset" "$work"
  cp "$src" "$resources/StatusIcon.png"
  local png="$work/icon-1024.png"
  sips -s format png -z 1024 1024 "$src" --out "$png" >/dev/null
  local size
  for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z $((size * 2)) $((size * 2)) "$png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
  done
  iconutil -c icns "$iconset" -o "$resources/AppIcon.icns"
  rm -rf "$work"
}

sign_app() {
  echo "Signing…"
  local framework="$APP/Contents/Frameworks/SoulverCore.framework"
  codesign --force --sign - --timestamp=none "$framework/Versions/A"
  codesign --force --sign - --timestamp=none "$framework"
  # inside out: helpers first, then the framework that seals them
  local sparkle="$APP/Contents/Frameworks/Sparkle.framework"
  codesign --force --sign - --timestamp=none -o runtime "$sparkle/Versions/B/Autoupdate"
  codesign --force --sign - --timestamp=none -o runtime "$sparkle/Versions/B/Updater.app"
  codesign --force --sign - --timestamp=none -o runtime "$sparkle/Versions/B"
  codesign --force --sign - --timestamp=none -o runtime "$sparkle"
  codesign --force --sign - --timestamp=none --identifier com.maxconine.qcalc "$BIN"
  codesign --force --sign - --timestamp=none --identifier com.maxconine.qcalc "$APP"
  codesign --verify --deep --strict "$APP"
}

copy_app() {
  local dest="$1"
  rm -rf "$dest"
  mkdir -p "$(dirname "$dest")"
  ditto "$APP" "$dest"
}

fetch_soulver
fetch_sparkle

mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"

ditto "$SLICE/SoulverCore.framework" "$APP/Contents/Frameworks/SoulverCore.framework"
# the xpc services are only for sandboxed apps
ditto "$SPARKLE/Sparkle.framework" "$APP/Contents/Frameworks/Sparkle.framework"
rm -rf "$APP/Contents/Frameworks/Sparkle.framework/XPCServices" \
  "$APP/Contents/Frameworks/Sparkle.framework/Versions/B/XPCServices"

# Apple Dictionary — to restore, add "$MAC/DictionaryLookup.swift" \ after SoulverEval.swift.
swiftc -parse-as-library \
  -O \
  -target "${ARCH}-apple-macos14.0" \
  -sdk "$(xcrun --sdk macosx --show-sdk-path)" \
  -F "$SLICE" \
  -F "$SPARKLE" \
  -framework SwiftUI \
  -framework AppKit \
  -framework WebKit \
  -framework Carbon \
  -framework SoulverCore \
  -framework Sparkle \
  -Xlinker -rpath -Xlinker @executable_path/../Frameworks \
  "$MAC/MathEval.swift" \
  "$MAC/SoulverEval.swift" \
  "$MAC/Overlay.swift" \
  "$MAC/UnitSettings.swift" \
  "$MAC/SettingsWindow.swift" \
  "$MAC/PeriodicWindow.swift" \
  "$MAC/Updates.swift" \
  "$MAC/QCalcApp.swift" \
  -o "$BIN"

cp "$MAC/Info.plist" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $APP_VERSION" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $APP_VERSION" "$APP/Contents/Info.plist"
set_update_key

echo "Building web assets…"
(cd "$ROOT" && npm run build)

rm -rf "$APP/Contents/Resources/web"
cp -R "$ROOT/dist" "$APP/Contents/Resources/web"

chmod +x "$BIN"
make_icon
sign_app

rm -rf "$MAC/dist"
copy_app "$DIST_APP"
echo "Built $DIST_APP ($APP_VERSION)"

if (( PACKAGE )); then
  rm -f "$DIST_ZIP"
  ditto -c -k --keepParent "$DIST_APP" "$DIST_ZIP"
  echo "Packaged $DIST_ZIP"
fi

if (( INSTALL )); then
  echo "Installing to /Applications/Q Calc.app…"
  rm -rf "/Applications/Instant Solver.app"
  copy_app "/Applications/Q Calc.app"
  echo "Installed /Applications/Q Calc.app"
fi
