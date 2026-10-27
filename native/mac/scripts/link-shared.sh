#!/bin/zsh
# (Re)creates Sources/Shared: symlinks to the macos/ files the native app compiles unchanged.
set -euo pipefail
MAC="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$MAC/Sources/Shared"
cd "$MAC/Sources/Shared"
# relinked in place, not removed and remade: a synced folder (iCloud Documents) answers a remade link with
# "Name 2.swift" copies, which then compile twice; any such copy goes
find . -maxdepth 1 -name '* [0-9]*.swift' -delete
for f in AppSettings Keybinds UnitlessCopy LoginItem Updates SheetPlace MessageInbox Messages ECBRates \
  SettingsWindow PeriodicWindow IdentityWindow OverlayHealth Compat; do
  ln -sfn "../../../../macos/$f.swift" "$f.swift"
done
