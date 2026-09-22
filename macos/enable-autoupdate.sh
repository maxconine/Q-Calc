#!/bin/zsh
set -euo pipefail

# Daily `brew upgrade` for Q Calc only. If autoupdate is already running, leave it
# alone so we do not overwrite a machine-wide schedule.

if [[ "${QCALC_NO_AUTOUPDATE:-}" == "1" ]]; then
  echo "Skipping Homebrew autoupdate."
  exit 0
fi

if ! command -v brew >/dev/null; then
  echo "Homebrew is required for autoupdates." >&2
  exit 1
fi

echo "Enabling daily Homebrew upgrades for Q Calc…"
brew tap domt4/autoupdate
if brew trust --help >/dev/null 2>&1; then
  brew trust --tap domt4/autoupdate
fi
if brew autoupdate status 2>/dev/null | grep -qi "running"; then
  echo "brew autoupdate is already running. Q Calc will update with your other casks on brew upgrade."
  exit 0
fi
brew autoupdate start --upgrade --immediate --only=q-calc
echo "Q Calc will check for Homebrew updates once a day and at login."
