// writes latest-native.json, the native app's update feed (native/windows/src/update.rs `offer` reads it), for the
// native installers of one release. the same shape and checks as the Tauri app's latest.json
// (src-tauri/release/latest-json.mjs): { version, pub_date, platforms: { "windows-x86_64": { signature, url } } },
// each signature the installer's .sig from `tauri signer sign`, its trusted comment naming exactly this version.
//
// usage: node latest-native.mjs <version> <download base url> <dir> platform=installer.exe ...
//   e.g. node latest-native.mjs 2.1.0 https://github.com/maxconine/Q-Calc/releases/download/v2.1.0 out \
//          windows-x86_64=Q-Calc-2.1.0-x64-native-setup.exe windows-aarch64=Q-Calc-2.1.0-arm64-native-setup.exe

import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { manifest } from '../../../src-tauri/release/latest-json.mjs'

export const FEED = 'latest-native.json'

export function nativeManifest(version, baseUrl, installers, now = new Date()) {
  for (const { file } of installers) {
    if (!/-native-setup\.exe$/.test(file)) throw new Error(`${file} isn't a native installer`)
  }
  return manifest(version, baseUrl, installers, now)
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [version, baseUrl, dir, ...pairs] = process.argv.slice(2)
  const installers = pairs.map((pair) => {
    const [platform, file] = pair.split('=')
    return { platform, file, sig: readFileSync(join(dir, `${file}.sig`), 'utf8') }
  })
  // no byte-order mark: update.rs takes one, but a plain file is what every other reader expects
  writeFileSync(join(dir, FEED), `${JSON.stringify(nativeManifest(version, baseUrl, installers), null, 2)}\n`)
  console.log(`${join(dir, FEED)}: ${installers.map((i) => i.platform).join(', ')}`)
}
