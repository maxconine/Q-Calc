#!/bin/zsh
set -euo pipefail

# Adds the zip to docs/appcast.xml for Sparkle. The EdDSA private key comes in on stdin
# and goes straight to sign_update, so it never touches disk or the log.
#   printf %s "$SPARKLE_PRIVATE_KEY" | zsh macos/update-appcast.sh <zip> <download-url>

if [[ $# -ne 2 ]]; then
  echo "Usage: macos/update-appcast.sh <zip> <download-url>  (private key on stdin)" >&2
  exit 1
fi

ZIP="$1"
URL="$2"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APPCAST="${QCALC_APPCAST:-$ROOT/docs/appcast.xml}"
SIGN_UPDATE="${SPARKLE_SIGN_UPDATE:-$(print -l "$ROOT"/macos/vendor/Sparkle-*/bin/sign_update(N) | tail -1)}"

if [[ ! -f "$ZIP" ]]; then
  echo "Missing $ZIP" >&2
  exit 1
fi
if [[ ! -f "$APPCAST" ]]; then
  echo "Missing $APPCAST" >&2
  exit 1
fi
if [[ -z "$SIGN_UPDATE" || ! -x "$SIGN_UPDATE" ]]; then
  echo "Missing Sparkle's sign_update; run macos/build.sh first." >&2
  exit 1
fi
if [[ -t 0 ]]; then
  echo "Pipe the EdDSA private key in on stdin." >&2
  exit 1
fi

# read versions from the app inside the zip, so the feed can't disagree with what ships
PLIST="$(mktemp)"
trap 'rm -f "$PLIST"' EXIT
unzip -p "$ZIP" "Q Calc.app/Contents/Info.plist" > "$PLIST"
field() { plutil -extract "$1" raw -o - "$PLIST"; }
BUILD="$(field CFBundleVersion)"
SHORT="$(field CFBundleShortVersionString)"
MIN_OS="$(field LSMinimumSystemVersion)"

if [[ ! "$BUILD" =~ '^[0-9]+(\.[0-9]+)*$' ]]; then
  echo "CFBundleVersion $BUILD must be plain dotted numbers for Sparkle to order it" >&2
  exit 1
fi

# prints: sparkle:edSignature="…" length="…"
ENCLOSURE="$("$SIGN_UPDATE" --ed-key-file - "$ZIP")"
if [[ ! "$ENCLOSURE" =~ '^sparkle:edSignature="[A-Za-z0-9+/=]+" length="[0-9]+"$' ]]; then
  echo "sign_update failed" >&2
  exit 1
fi

BUILD="$BUILD" SHORT="$SHORT" MIN_OS="$MIN_OS" URL="$URL" ENCLOSURE="$ENCLOSURE" \
  PUB_DATE="$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S +0000')" \
  python3 - "$APPCAST" <<'PY'
import os, re, sys
from xml.sax.saxutils import escape, quoteattr

path = sys.argv[1]
feed = open(path, encoding="utf-8").read()
env = os.environ
build = env["BUILD"]

def key(v):
    return tuple(int(p) for p in v.split("."))

# a rerun of the same release replaces its item (the zip, and so the signature, changed)
items = re.findall(r"[ \t]*<item>.*?</item>\n", feed, re.S)
for item in items:
    m = re.search(r"<sparkle:version>([^<]+)</sparkle:version>", item)
    if not m:
        continue
    if m.group(1) == build:
        feed = feed.replace(item, "", 1)
    elif key(m.group(1)) > key(build):
        sys.exit(f"appcast already has build {m.group(1)}, newer than {build}")

item = f"""    <item>
      <title>Version {escape(env["SHORT"])}</title>
      <pubDate>{env["PUB_DATE"]}</pubDate>
      <sparkle:version>{escape(build)}</sparkle:version>
      <sparkle:shortVersionString>{escape(env["SHORT"])}</sparkle:shortVersionString>
      <sparkle:minimumSystemVersion>{escape(env["MIN_OS"])}</sparkle:minimumSystemVersion>
      <enclosure url={quoteattr(env["URL"])} {env["ENCLOSURE"]} type="application/octet-stream"/>
    </item>
"""
marker = "  </channel>"
at = feed.find("    <item>")
if at < 0:
    at = feed.find(marker)
if at < 0:
    sys.exit("appcast has no <channel>")
feed = feed[:at] + item + feed[at:]
open(path, "w", encoding="utf-8").write(feed)
PY

xmllint --noout "$APPCAST"
echo "Added $SHORT ($BUILD) to $APPCAST"
