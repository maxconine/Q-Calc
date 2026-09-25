// writes the updater manifest (latest.json) for the windows installers of one release.
// usage: node latest-json.mjs <version> <download base url> <dir> platform=installer.exe ...
// each installer needs its .sig next to it, signed for exactly this version.

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

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [version, baseUrl, dir, ...pairs] = process.argv.slice(2)
  const installers = pairs.map((pair) => {
    const [platform, file] = pair.split('=')
    return { platform, file, sig: readFileSync(join(dir, `${file}.sig`), 'utf8') }
  })
  writeFileSync(join(dir, 'latest.json'), `${JSON.stringify(manifest(version, baseUrl, installers), null, 2)}\n`)
}
