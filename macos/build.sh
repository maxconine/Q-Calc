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
ONLY_ARCH=""
APP_VERSION="$(node -p "require('$ROOT/package.json').version")"

for arg in "$@"; do
  case "$arg" in
    --install) INSTALL=1 ;;
    --package) PACKAGE=1 ;;
    --arch=*) ONLY_ARCH="${arg#--arch=}" ;;
    -h|--help)
      echo "Usage: macos/build.sh [--install] [--package] [--arch=arm64|x86_64]"
      echo "  --install   copy Q Calc.app into /Applications"
      echo "  --package   build both and write macos/dist/Q-Calc-<version>-apple-silicon.zip and -intel.zip"
      echo "              for release (needs the real SUPublicEDKey)"
      echo "  --arch=…    build only this architecture (default: this Mac's, or both with --package)"
      echo "  QCALC_SPARKLE_PUBLIC_KEY=<key> overrides SUPublicEDKey, for update tests"
      exit 0
      ;;
    *)
      echo "Unknown option: $arg" >&2
      echo "Usage: macos/build.sh [--install] [--package] [--arch=arm64|x86_64]" >&2
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

# a separate app per architecture, each carrying only its own slices, back to ventura. 13.5 is soulvercore's own minimum
MIN_MACOS="13.5"
HOST_ARCH="$(uname -m)"
if [[ -n "$ONLY_ARCH" ]]; then
  ARCHS=("$ONLY_ARCH")
elif (( PACKAGE )); then
  ARCHS=(arm64 x86_64)
else
  ARCHS=("$HOST_ARCH")
fi
for arch in "${ARCHS[@]}"; do
  if [[ "$arch" != arm64 && "$arch" != x86_64 ]]; then
    echo "Unknown architecture: $arch (use arm64 or x86_64)" >&2
    exit 1
  fi
done
# the name on the download, and each build's own update feed: apple silicon keeps the feed installed copies read
label() { [[ "$1" == arm64 ]] && echo apple-silicon || echo intel; }
feed() { [[ "$1" == arm64 ]] && echo appcast.xml || echo appcast-intel.xml; }

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

# once per run, into $STAGE/icon; each app copies it
make_icon() {
  local resources="$STAGE/icon"
  mkdir -p "$resources"
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
  # ditto carries FinderInfo and File Provider xattrs over from the synced Documents folder
  xattr -cr "$APP"
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

echo "Building web assets…"
(cd "$ROOT" && npm run build)
make_icon

# a fat binary carries every architecture; each app keeps only its own
thin_app() {
  local arch="$1" file archs
  find "$APP" -type f -print0 | while IFS= read -r -d '' file; do
    archs="$(lipo -archs "$file" 2>/dev/null)" || continue
    [[ "$archs" == *" "* ]] || continue
    lipo -thin "$arch" "$file" -output "$file.thin"
    chmod "$(stat -f %Lp "$file")" "$file.thin"
    mv "$file.thin" "$file"
  done
}

# Apple Dictionary — to restore, add "$MAC/DictionaryLookup.swift" \ after SoulverEval.swift.
build_app() {
  local arch="$1"
  APP="$STAGE/$arch/Q Calc.app"
  BIN="$APP/Contents/MacOS/QCalc"
  echo "Building for $arch…"
  mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"

  ditto "$SLICE/SoulverCore.framework" "$APP/Contents/Frameworks/SoulverCore.framework"
  # the xpc services are only for sandboxed apps
  ditto "$SPARKLE/Sparkle.framework" "$APP/Contents/Frameworks/Sparkle.framework"
  rm -rf "$APP/Contents/Frameworks/Sparkle.framework/XPCServices" \
    "$APP/Contents/Frameworks/Sparkle.framework/Versions/B/XPCServices"

  swiftc -parse-as-library \
    -O \
    -target "${arch}-apple-macos${MIN_MACOS}" \
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
    -Xlinker -dead_strip \
    -module-cache-path "$STAGE/modules-$arch" \
    "$MAC/MathEval.swift" \
    "$MAC/SoulverEval.swift" \
    "$MAC/Overlay.swift" \
    "$MAC/UnitSettings.swift" \
    "$MAC/Keybinds.swift" \
    "$MAC/SettingsWindow.swift" \
    "$MAC/PeriodicWindow.swift" \
    "$MAC/Updates.swift" \
    "$MAC/QCalcApp.swift" \
    -o "$BIN"
  # local symbols only help a debugger
  strip -x "$BIN"

  cp "$MAC/Info.plist" "$APP/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $APP_VERSION" "$APP/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $APP_VERSION" "$APP/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Set :SUFeedURL https://maxconine.github.io/Q-Calc/$(feed "$arch")" "$APP/Contents/Info.plist"
  set_update_key

  cp -R "$ROOT/dist" "$APP/Contents/Resources/web"
  cp "$STAGE/icon/"* "$APP/Contents/Resources/" 2>/dev/null || true

  thin_app "$arch"
  chmod +x "$BIN"
  sign_app
}

rm -rf "$MAC/dist"
for arch in "${ARCHS[@]}"; do
  build_app "$arch"
  if (( ${#ARCHS[@]} > 1 )); then
    dest="$MAC/dist/$(label "$arch")/Q Calc.app"
  else
    dest="$DIST_APP"
  fi
  copy_app "$dest"
  echo "Built $dest ($APP_VERSION, $arch)"
  if (( PACKAGE )); then
    zip="$MAC/dist/Q-Calc-${APP_VERSION}-$(label "$arch").zip"
    rm -f "$zip"
    # from the stage: a copy in a synced Documents folder picks up xattrs that break the signature
    ditto -c -k --norsrc --keepParent "$APP" "$zip"
    echo "Packaged $zip ($(du -h "$zip" | cut -f1))"
  fi
  # this Mac's build is the one that installs
  if [[ "$arch" == "$HOST_ARCH" ]]; then
    INSTALL_APP="$dest"
  fi
done

if (( INSTALL )); then
  if [[ -z "${INSTALL_APP:-}" ]]; then
    echo "Nothing built for this Mac ($HOST_ARCH) to install." >&2
    exit 1
  fi
  echo "Installing to /Applications/Q Calc.app…"
  rm -rf "/Applications/Instant Solver.app"
  APP="$INSTALL_APP"
  copy_app "/Applications/Q Calc.app"
  echo "Installed /Applications/Q Calc.app"
fi
