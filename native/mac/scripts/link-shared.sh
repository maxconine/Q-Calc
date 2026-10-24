#!/bin/zsh
# (Re)creates Sources/Shared: symlinks to the macos/ files the native app compiles unchanged.
set -euo pipefail
MAC="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$MAC/Sources/Shared"
cd "$MAC/Sources/Shared"
for f in AppSettings Keybinds UnitlessCopy LoginItem Updates SheetPlace MessageInbox Messages ECBRates \
  UnitSettings SettingsWindow PeriodicWindow IdentityWindow OverlayHealth; do
  ln -sfn "../../../../macos/$f.swift" "$f.swift"
done
