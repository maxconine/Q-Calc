# Windows releases

Pushing a `v*` tag runs two separate workflows. `release.yml` builds the Mac zip and creates the GitHub Release. `windows.yml` builds the Windows installers, waits for that release to exist, and then attaches:

- `Q-Calc-<version>-x64-setup.exe`
- `Q-Calc-<version>-arm64-setup.exe` (skipped for that release if the arm64 build fails)
- `latest.json`, the manifest the in-app updater reads

If the Windows workflow fails, the Mac release is unaffected.

Running **Windows** by hand with *publish* off builds unsigned installers and saves them as workflow artefacts. This replaces the old manual build. With *publish* on, it does the full release for the `package.json` version.

## For a Windows user

1. They download `Q-Calc-<version>-x64-setup.exe` from the release page, or run `winget install maxconine.QCalc`.
2. The installer installs for the current user only, so there's no admin prompt. It goes to `%LOCALAPPDATA%\Q Calc`, adds a Start menu entry, and appears in *Installed apps* with an uninstaller. If WebView2 is missing, which is rare on Windows 10 and 11, the installer quietly fetches Microsoft's small bootstrapper.
3. Q Calc starts hidden in the tray. Alt+Space opens it.
4. About 20 seconds after launch, and then once a day, the app checks `https://github.com/maxconine/Q-Calc/releases/latest/download/latest.json`. If there's a newer version, it downloads the installer in the background and verifies its signature against the public key built into the app.
5. The update installs once two things are true: the calculator has been hidden for 5 minutes, and there has been no keyboard or mouse input for 5 minutes. The installer runs silently and relaunches Q Calc hidden. The user sees no dialogs. At most, the hotkey doesn't respond for a few seconds while they're away.
6. If a check fails (offline, rate-limited, or a release without `latest.json`), the app tries again an hour later.

The update logic is in `src-tauri/src/update.rs`. It's turned off in debug builds, and it stays off while the public key is still the placeholder.

## One-time setup to go live

A repo admin (maxconine) needs to do this once.

1. **Make the updater key**, on any machine with the repo checked out:

   ```sh
   npx tauri signer generate -w ~/.tauri/qcalc-updater.key
   ```

   Pick a strong password. This writes `qcalc-updater.key` (private) and `qcalc-updater.key.pub`. Back up the private key and its password in a password manager. **If either is lost, installed copies can never be updated again.** You would have to ship a manual reinstall.

2. **Put the public key in the app.** Paste the whole contents of `qcalc-updater.key.pub` in place of `QCALC_UPDATER_PUBKEY_PLACEHOLDER` in `src-tauri/tauri.windows.conf.json`, then commit. Until you do this, publishing fails on purpose with a clear error.

3. **Add the repository secrets** under Settings → Secrets and variables → Actions:

   | Secret | Value |
   | --- | --- |
   | `TAURI_SIGNING_PRIVATE_KEY` | the whole contents of `qcalc-updater.key` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password |

4. Tag a release as usual (see the root `.gitignore` notes).

## Code signing (optional)

Without Authenticode, the updater still works: silent updates never show SmartScreen. The first manual install does show *Windows protected your PC*, where the user clicks **More info → Run anyway**. The workflow signs only when one of these is configured. Azure takes priority if both are.

**Azure Artifact Signing** (formerly Trusted Signing). This is the recommended option.
- It costs about US$10 a month (Basic tier, 5,000 signatures), and identity validation is required.
- At the time of writing, individuals need to be in the US or Canada. Organisations need a few years of history.
- It signs the app, the uninstaller and the installer during the build.
- Setup:
  1. Create an Artifact Signing account and a public-trust certificate profile.
  2. Create an app registration with the *Artifact Signing Certificate Profile Signer* role.
  3. Add secrets `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET` and `AZURE_TENANT_ID`.
  4. Add repository variables `AZURE_SIGNING_ENDPOINT` (for example `https://eus.codesigning.azure.net`), `AZURE_SIGNING_ACCOUNT` and `AZURE_SIGNING_PROFILE`.

**SignPath Foundation**
- It's free for open-source projects that qualify (Q Calc is MIT). You apply, and they review the project.
- The certificate is issued to SignPath Foundation, not to you. Each release may need a manual approval click in SignPath.
- It signs the finished installer, but not the app inside it.
- Setup:
  1. Apply, then create a project with the GitHub trusted build system. Its artifact configuration is a `<zip-file>` holding a `<pe-file>`.
  2. Add secret `SIGNPATH_API_TOKEN`.
  3. Add variables `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG` and `SIGNPATH_POLICY_SLUG`.

A traditional OV certificate (about US$200 to 400 a year) needs a hardware token or a cloud HSM, which fits CI poorly. It's not wired up.

The updater signature is always made after any Authenticode signing, so it covers the final bytes.

## winget

Each published release saves a `winget-manifests` artefact on the workflow run. It holds the version, locale and installer manifests with the right URLs and SHA-256 hashes. To submit:

1. Download the artefact and run `winget validate --manifest <folder>` on Windows.
2. Optionally, test it with `winget install --manifest <folder>`.
3. Fork `microsoft/winget-pkgs` and add the folder under `manifests/m/maxconine/QCalc/<version>/`. Open a pull request.

`wingetcreate submit <folder>` does step 3 for you. Once the first version is accepted, later versions can be submitted the same way. Winget users also get the in-app silent updates, so falling behind on winget matters little.

## Checking an update by hand

On a Windows test machine:

1. Build and install version N from a real key.
2. Publish N+1.
3. Leave Q Calc hidden and don't touch the machine for about 6 minutes after its first check.
4. It should restart as N+1. You can confirm the new version under *Installed apps*.
