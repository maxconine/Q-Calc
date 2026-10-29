// writes the updater manifest (latest.json) for the windows installers of one release.
// usage: node latest-json.mjs [--keep <old latest.json>] <version> <download base url> <dir> platform=installer.exe ...
// each installer needs its .sig next to it, signed for exactly this version.
//
// the installers can be the Tauri app's (Q-Calc-<v>-x64-setup.exe) or, from 3.0 on, the native app's
// (Q-Calc-<v>-x64-native-setup.exe): the native installer upgrades a Tauri install in place, so pointing these
// entries at it moves every Tauri install over to the native app (windows.yml NATIVE_WINDOWS).
//
// only the plain windows-x86_64 / windows-aarch64 keys are written. tauri-plugin-updater looks for
// windows-<arch>-nsis before windows-<arch>, so an -nsis key left in would win over them: --keep carries over
// the other platforms of an older manifest (a darwin-* entry, say) but never a windows-* one.

import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

export const PLATFORMS = ['windows-x86_64', 'windows-aarch64']

// the .sig file is base64 of a minisign signature; its trusted comment carries version:<v>
export function signedVersion(sig) {
  const text = Buffer.from(sig.trim(), 'base64').toString('utf8')
  const comment = text.split('\n').find((line) => line.startsWith('trusted comment: '))
  if (!comment) return null
  const field = comment
    .slice('trusted comment: '.length)
    .split('\t')
    .find((part) => part.startsWith('version:'))
  return field ? field.slice('version:'.length).trim() : null
}

export function manifest(version, baseUrl, installers, now = new Date()) {
  if (!/^\d+\.\d+\.\d+([-+].*)?$/.test(version)) throw new Error(`not a version: ${version}`)
  if (!installers.some((i) => i.platform === 'windows-x86_64')) throw new Error('the x64 installer is missing')
  const platforms = {}
  for (const { platform, file, sig } of installers) {
    if (!PLATFORMS.includes(platform)) throw new Error(`unknown platform: ${platform}`)
    const signed = signedVersion(sig)
    if (signed !== version) throw new Error(`${file} is signed for ${signed ?? 'no version'}, not ${version}`)
    platforms[platform] = { signature: sig.trim(), url: `${baseUrl.replace(/\/$/, '')}/${encodeURIComponent(file)}` }
  }
  return { version, pub_date: now.toISOString().replace(/\.\d{3}Z$/, 'Z'), platforms }
}

// manifest() plus the non-windows platforms of `previous` (an older latest.json), as they were
export function compose(version, baseUrl, installers, previous = null, now = new Date()) {
  const next = manifest(version, baseUrl, installers, now)
  const kept = Object.entries(previous?.platforms ?? {}).filter(([key]) => !key.startsWith('windows-'))
  next.platforms = { ...next.platforms, ...Object.fromEntries(kept) }
  return next
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2)
  let previous = null
  if (args[0] === '--keep') {
    previous = JSON.parse(readFileSync(args[1], 'utf8').replace(/^﻿/, ''))
    args.splice(0, 2)
  }
  const [version, baseUrl, dir, ...pairs] = args
  const installers = pairs.map((pair) => {
    const [platform, file] = pair.split('=')
    return { platform, file, sig: readFileSync(join(dir, `${file}.sig`), 'utf8') }
  })
  const out = compose(version, baseUrl, installers, previous)
  writeFileSync(join(dir, 'latest.json'), `${JSON.stringify(out, null, 2)}\n`)
  console.log(`${join(dir, 'latest.json')}: ${Object.keys(out.platforms).join(', ')}`)
}
