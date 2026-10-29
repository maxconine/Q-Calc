# Shipping a Windows release (v3.0.0: the native app)

From 3.0 the Windows release is the native app. A `v*` tag builds the native installers. `latest.json`, the feed every Tauri install (2.0.x) reads, then points at those installers, so each Tauri install updates itself to the native app within a day. The native installer replaces the Tauri app in place: same folder (`%LOCALAPPDATA%\Q Calc`), same **Installed apps** entry, same Start menu shortcut and sign-in entry. Settings, keybinds and rates live in `%APPDATA%\com.maxconine.qcalc`, which both apps share, and `migrate.rs` moves the Tauri app's history at the first launch.

The switch is `NATIVE_WINDOWS` in `.github/workflows/windows.yml`. A tag push always takes it. A hand run has a *native_windows* box, ticked by default.

## What the release will hold

Made by `release.yml` (Mac, unchanged):
- `Q-Calc-3.0.0-apple-silicon.zip`, `Q-Calc-3.0.0-intel.zip`. The bot also commits `Casks/q-calc.rb`, `docs/appcast.xml` and `docs/appcast-intel.xml` to `main`, so Macs update to this build through Sparkle and Homebrew. See *Open questions* below.

Added by `windows.yml`:
- `Q-Calc-3.0.0-x64-native-setup.exe`
- `Q-Calc-3.0.0-arm64-native-setup.exe` (cross-built on the x64 runner; if only that build fails, x64 still ships and Arm PCs stay on 2.0.15 until the next release)
- `latest-native.json`: the native app's feed (`windows-x86_64`, `windows-aarch64`)
- `latest.json`: the Tauri app's feed. It has only the `windows-x86_64` and `windows-aarch64` keys, both pointing at the native installers. It has no `-nsis` keys, which tauri-plugin-updater would otherwise prefer.
- the release notes from `RELEASE_NOTES_v3.0.0.md` (replacing the generated ones)
- a `winget-manifests` workflow artefact (not on the release), for the native installers

No Tauri installers are built for 3.0.0.

## The evening before (done on branch `v3-release`)

- [x] Version 3.0.0: `package.json` (and its lock), `native/windows/Cargo.toml` (the crate version a plain build reports; the release passes `QCALC_VERSION=3.0.0` as well), `src-tauri/Cargo.toml`. `windows.yml` refuses a tag where package.json and native/windows/Cargo.toml differ.
- [x] Local dry run: build.ps1 at 3.0.0 (x64), signed with a throwaway key, both feeds composed and checked with `verify-feed.mjs`, then `feed-check.ps1` and `update-e2e.ps1 -New 3.0.0`.
- [x] Download page (`docs/index.html`, Windows paragraph), README, release notes.

## In the morning

### 1. Check by hand (once)

- Settings → Secrets and variables → Actions has `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (the updater key whose public half is in `src-tauri/tauri.windows.conf.json` and `native/windows/src/update.rs`; 2.0.15 was signed with it), and `SPARKLE_PRIVATE_KEY` for the Mac job. The native job stops with a plain error if the updater key is missing.
- The real signing key and its password stay with you. Nothing in this branch has it, and the dry run used a throwaway key.
- Authenticode code signing stays off on purpose: there's no certificate, and `scripts/sign-check.ps1` is the mock. The installers are unsigned, which the download page and release notes say.
- Decide the Mac question below **before tagging**. The tag ships the Mac too.

### 2. Optional: a build-only run (no publishing)

This proves the arm64 cross build on the runner before the tag. Go to Actions → **Windows** → Run workflow on the branch you'll tag, with *publish* **off** and *native_windows* **on**. With *publish* off nothing is signed and the publish job doesn't run. Expect `native x64` and `native arm64` to go green (the *Check the exe's architecture* step prints `machine AA64 (arm64)`) and two artefacts, `q-calc-windows-native-x64` and `-arm64`.

### 3. Merge and tag

The tag must be on a commit that has this branch. The Mac bot pushes to `main`, so tag `main`:

```bash
git checkout windows-native && git merge v3-release      # the coordinator's usual merge
git checkout main && git pull --ff-only && git merge windows-native
node -p "require('./package.json').version"                 # 3.0.0
grep -m1 '^version' native/windows/Cargo.toml               # version = "3.0.0"
git tag v3.0.0
git push origin main v3.0.0
```

### 4. Watch (about 30 to 40 minutes)

- **Release** (mac, about 15 minutes): creates `v3.0.0` with the two zips, then pushes `q-calc 3.0.0` (cask and appcasts) to `main`.
- **Windows**: the `build x64` / `build arm64` (Tauri) jobs show as *skipped*. That's expected.
  - `native x64` must be green. `native arm64` should be. If arm64 fails, the publish log says `No native arm64 installer this time`.
  - `publish`, step *Collect installers*: `windows is the native app: windows-x86_64=Q-Calc-3.0.0-x64-native-setup.exe windows-aarch64=Q-Calc-3.0.0-arm64-native-setup.exe`
  - *Check the feeds*: two lines per feed, `... windows x86_64 -> windows-x86_64 -> Q-Calc-3.0.0-x64-native-setup.exe (signature ok)` and the same for aarch64. Nothing is uploaded unless this passes.
  - *Wait for the release* waits for the Mac job (up to an hour).
  - *Upload to the release*: the installers go first, then `latest-native.json`, then `latest.json`.
- When both are done:

```bash
gh release view v3.0.0 --json assets --jq '.assets[].name'
curl -sL https://github.com/maxconine/Q-Calc/releases/latest/download/latest.json          # urls end -native-setup.exe
curl -sL https://github.com/maxconine/Q-Calc/releases/latest/download/latest-native.json
```

### 5. Try it on a real PC

- **Fresh install:** download `Q-Calc-3.0.0-x64-native-setup.exe` from the release page and run it (More info → Run anyway). Alt+Space should open the bar, and Settings › Updates should say Q Calc 3.0.0 and up to date.
- **Upgrade:** on a PC with Tauri 2.0.15, open Settings and click Check for updates (or wait for the daily check). Leave the PC alone for about 6 minutes with the bar hidden. Q Calc comes back in the tray as the native app. Check that **Installed apps** says 3.0.0 and that history and settings are there.
- Arm PC, if you have one: the same with the arm64 installer.

### 6. winget (by hand, when happy)

Download the `winget-manifests` artefact from the publish run. Run `winget validate --manifest manifests/m/maxconine/QCalc/3.0.0`. Optionally test with `winget install --manifest <folder>`. Then submit with `wingetcreate submit <folder>`, or open a PR to `microsoft/winget-pkgs`. The manifests point at `.../releases/download/v3.0.0/Q-Calc-3.0.0-<arch>-native-setup.exe` with their SHA-256, and set `MinimumOSVersion: 10.0.10240.0`.

### 7. Docs

`docs/index.html` already links the 3.0.0 native installers. Pages serves it from `main`, so the links work once the Windows job has uploaded. Between the push and the upload, the Windows links on the download page 404.

## If something goes wrong

Every step here is a `gh` command against the release. Nothing needs a rebuild unless it says so.

| Problem | Do this | Effect |
| --- | --- | --- |
| The native build fails before publishing | Fix it on a branch, merge, then delete and re-push the tag (`git push --delete origin v3.0.0`, `git tag -f v3.0.0`, `git push origin v3.0.0`), or re-run the failed jobs | Nothing was uploaded, and `latest.json` is still 2.0.15's, so no one has updated |
| **Stop Tauri installs moving to the native app** (keep the release) | `gh release download v2.0.15 -p latest.json -D hold` then `gh release upload v3.0.0 hold/latest.json --clobber` | `latest.json` says 2.0.15, so Tauri installs see nothing newer and stay put. Native installs are unaffected (they read `latest-native.json`) |
| Same, quicker | `gh release delete-asset v3.0.0 latest.json -y` | Tauri installs get a 404, show "failed" in Settings, and retry hourly. Harmless, but noisier |
| **Send Tauri installs to a Tauri 3.0.0 instead** | Actions → Windows → Run workflow on the tagged commit, *publish* on, *native_windows* **off** | Builds the Tauri installers and points `latest.json` at them (it re-uploads the native installers and `latest-native.json` too). Re-pointing at native later: run again with *native_windows* on |
| Re-point `latest.json` by hand at any signed installers on the release | Put the installers and their `.sig` files in a folder, then run `node src-tauri/release/latest-json.mjs 3.0.0 https://github.com/maxconine/Q-Calc/releases/download/v3.0.0 <dir> windows-x86_64=<x64.exe> windows-aarch64=<arm64.exe>`, then `node src-tauri/release/verify-feed.mjs <dir>/latest.json --dir <dir> --pubkey-from src-tauri/tauri.windows.conf.json`, then `gh release upload v3.0.0 <dir>/latest.json --clobber` | The `.sig` files are in the workflow's `q-calc-windows-native-*` artefacts |
| **The native app is broken for people who already moved** | Ship 3.0.1 with the fix: bump `package.json` and `native/windows/Cargo.toml`, then tag | Updates only go forward (a newer version is required), so a fix is the only clean way back. Hold `latest.json` (above) meanwhile |
| Pull the whole release back | `gh release edit v2.0.15 --latest` | `releases/latest/download/...` serves 2.0.15 again: `latest.json` is 2.0.15's (no update), and `latest-native.json` 404s (native installs read "up to date"). Macs: revert the bot's `q-calc 3.0.0` commit on `main` (cask and appcasts) |

## Open questions for the owner

- **The Mac ships with this tag.** `release.yml` builds the Swift app in `macos/` at package.json's version (`build.sh` stamps it into Info.plist), so the tag makes Mac 3.0.0 zips. It also updates the cask and the appcasts on `main`, so every Mac updates. `main` has Mac changes since v2.0.15 (messages, clips and files, macOS 12 support), so that's a real Mac update, not only a new number. This branch leaves the Mac job, the Mac links on the download page (still 2.0.15, which keep working) and `macos/Info.plist` (2.0.15; the build overrides it) as they are. Options:
  1. Ship Mac 3.0.0 as it is on `main`. Then bump the Mac links in `docs/index.html` and the "current public release" line in README.md to 3.0.0.
  2. Keep the Mac on 2.0.15. Add a guard to `release.yml` that skips the zips, cask and appcast but still creates the release, because `windows.yml` waits for it.
  3. Wait for the native Mac app (`native/mac` on `origin/native`) to be a v3 too. That would need its own packaging (signing, notarisation if wanted, a Sparkle feed and cask pointing at it) and a migration from the Swift app's defaults.
- **Text messages** aren't end-to-end encrypted (clips and files are; see `native/PRIVACY.md`). The release notes say so plainly.
- **Code signing**: still unsigned, so SmartScreen appears on a manual install and Smart App Control blocks the app. Silent updates don't trigger SmartScreen. When a certificate exists, pass `-SignCommand` to build.ps1 in the native job. The updater `.sig` is made after it, so it covers the signed bytes.
