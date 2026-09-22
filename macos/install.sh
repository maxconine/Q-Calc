#!/bin/zsh
set -euo pipefail

TAP="maxconine/qcalc"
REPO="https://github.com/maxconine/Instant-Calculator.git"
AUTOUPDATE=1

for arg in "$@"; do
  case "$arg" in
    --no-autoupdate) AUTOUPDATE=0 ;;
    -h|--help)
      echo "Usage: macos/install.sh [--no-autoupdate]"
      echo "  Install Q Calc with Homebrew and optionally enable daily upgrades."
      exit 0
      ;;
    *)
      echo "Unknown option: $arg" >&2
      echo "Usage: macos/install.sh [--no-autoupdate]" >&2
      exit 1
      ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Q Calc is a Mac app." >&2
  exit 1
fi

if [[ "$(uname -m)" != "arm64" ]]; then
  echo "Q Calc requires an Apple silicon Mac (M1 or later)." >&2
  exit 1
fi

if ! command -v brew >/dev/null; then
  echo "Install Homebrew first: https://brew.sh" >&2
  exit 1
fi

echo "Tapping ${TAP}…"
brew tap "$TAP" "$REPO"
if brew trust --help >/dev/null 2>&1; then
  brew trust --tap "$TAP"
fi

echo "Installing Q Calc…"
brew install --cask --force --yes q-calc
xattr -cr "/Applications/Q Calc.app"

if (( AUTOUPDATE )); then
  echo "Enabling daily Homebrew upgrades for Q Calc…"
  brew tap domt4/autoupdate
  if brew autoupdate status 2>/dev/null | grep -qi "running"; then
    echo "brew autoupdate is already running. Q Calc will update with your other casks on brew upgrade."
  else
    brew autoupdate start --upgrade --immediate --only=q-calc
  fi
fi

echo "Opening Q Calc…"
open "/Applications/Q Calc.app"

echo "Installed. Later updates: brew upgrade --cask q-calc"
