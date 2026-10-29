// tauri-upgrade.ps1's way into the Tauri app's page: evaluates an expression in a WebView2 page over the DevTools
// protocol (the web view started with --remote-debugging-port, loopback only) and prints the result as JSON.
//   node cdp-eval.mjs <port> <url substring> <expression>
// exits 3 while no page matches yet (the caller retries), 1 on an error in the page.

const [port, match, expression] = process.argv.slice(2)
if (!port || !match || !expression) {
  console.error('usage: node cdp-eval.mjs <port> <url substring> <expression>')
  process.exit(2)
}

let targets
try {
  targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
} catch {
  process.exit(3)
}
// the bar's page (index.html) before any other window's (settings.html, sheet.html, quick.html)
const pages = targets.filter((t) => t.type === 'page' && t.url.includes(match) && t.webSocketDebuggerUrl)
const page = pages.find((t) => ['/', '/index.html'].includes(new URL(t.url).pathname)) ?? pages[0]
if (!page) process.exit(3)

const ws = new WebSocket(page.webSocketDebuggerUrl)
const timer = setTimeout(() => {
  console.error('timed out')
  process.exit(1)
}, 15000)
ws.onopen = () => ws.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, returnByValue: true, awaitPromise: true } }))
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data)
  if (msg.id !== 1) return
  clearTimeout(timer)
  const r = msg.result
  if (!r || r.exceptionDetails) {
    console.error(JSON.stringify(r?.exceptionDetails ?? msg.error))
    process.exit(1)
  }
  console.log(JSON.stringify({ url: page.url, value: r.result.value ?? null }))
  ws.close()
  process.exit(0)
}
