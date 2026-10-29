// update-e2e.ps1's stand-in for GitHub's release downloads: a loopback-only http server over one folder.
//   node update-server.mjs <folder> [port]
// prints `port <n>` once listening, then one line per request: `<method> <path> <status> <bytes>`.
// - /<file>: the file (404 when it isn't there, like a release without latest-native.json);
// - /drop/<file>: the headers say the whole file, then the connection drops halfway (a download cut off).
// it binds 127.0.0.1 only, so nothing off this machine can reach it.

import { createServer } from 'node:http'
import { readFileSync, existsSync, statSync } from 'node:fs'
import { join, normalize, sep } from 'node:path'

const [root, portArg] = process.argv.slice(2)
if (!root) {
  console.error('usage: node update-server.mjs <folder> [port]')
  process.exit(2)
}

const server = createServer((req, res) => {
  const url = new URL(req.url, 'http://127.0.0.1')
  let path = decodeURIComponent(url.pathname)
  const drop = path.startsWith('/drop/')
  if (drop) path = path.slice('/drop'.length)
  const file = normalize(join(root, path))
  const log = (status, bytes) => console.log(`${req.method} ${url.pathname} ${status} ${bytes}`)
  if (!file.startsWith(normalize(root) + sep) || !existsSync(file) || !statSync(file).isFile()) {
    res.writeHead(404, { 'content-type': 'text/plain' })
    res.end('Not Found')
    log(404, 0)
    return
  }
  const body = readFileSync(file)
  res.writeHead(200, { 'content-type': 'application/octet-stream', 'content-length': body.length })
  if (drop) {
    res.write(body.subarray(0, Math.floor(body.length / 2)), () => {
      log(200, Math.floor(body.length / 2))
      req.socket.destroy()
    })
    return
  }
  res.end(body)
  log(200, body.length)
})

server.listen(Number(portArg ?? 0), '127.0.0.1', () => {
  console.log(`port ${server.address().port}`)
})
