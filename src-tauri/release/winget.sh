#!/usr/bin/env bash
# writes the winget manifests for one release, ready for a pull request to microsoft/winget-pkgs.
# usage: winget.sh <version> <download base url> <dir with installers> <out dir>
set -euo pipefail

VERSION="$1"
BASE="${2%/}"
DIR="$3"
OUT="$4/manifests/m/maxconine/QCalc/${VERSION}"
ID="maxconine.QCalc"
SCHEMA="1.10.0"
mkdir -p "$OUT"

installer() {
  local arch="$1" file="$2"
  [[ -f "${DIR}/${file}" ]] || return 0
  local sha
  sha="$(sha256sum "${DIR}/${file}" | awk '{print toupper($1)}')"
  cat <<YAML
  - Architecture: ${arch}
    InstallerUrl: ${BASE}/${file}
    InstallerSha256: ${sha}
YAML
}

cat > "${OUT}/${ID}.yaml" <<YAML
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.version.${SCHEMA}.schema.json
PackageIdentifier: ${ID}
PackageVersion: ${VERSION}
DefaultLocale: en-US
ManifestType: version
ManifestVersion: ${SCHEMA}
YAML

cat > "${OUT}/${ID}.locale.en-US.yaml" <<YAML
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.defaultLocale.${SCHEMA}.schema.json
PackageIdentifier: ${ID}
PackageVersion: ${VERSION}
PackageLocale: en-US
Publisher: maxconine
PublisherUrl: https://github.com/maxconine
PackageName: Q Calc
PackageUrl: https://github.com/maxconine/Q-Calc
License: MIT
LicenseUrl: https://github.com/maxconine/Q-Calc/blob/main/LICENSE
ShortDescription: A Spotlight-style scientific calculator. Press Alt+Space, type, get the answer.
Moniker: qcalc
Tags:
  - calculator
  - math
  - units
ReleaseNotesUrl: https://github.com/maxconine/Q-Calc/releases/tag/v${VERSION}
ManifestType: defaultLocale
ManifestVersion: ${SCHEMA}
YAML

{
  cat <<YAML
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.installer.${SCHEMA}.schema.json
PackageIdentifier: ${ID}
PackageVersion: ${VERSION}
InstallerType: nullsoft
Scope: user
InstallModes:
  - silent
  - silentWithProgress
UpgradeBehavior: install
ProductCode: Q Calc
ReleaseDate: $(date -u +%Y-%m-%d)
Installers:
YAML
  installer x64 "Q-Calc-${VERSION}-x64-setup.exe"
  installer arm64 "Q-Calc-${VERSION}-arm64-setup.exe"
  cat <<YAML
ManifestType: installer
ManifestVersion: ${SCHEMA}
YAML
} > "${OUT}/${ID}.installer.yaml"

echo "winget manifests in ${OUT}"
