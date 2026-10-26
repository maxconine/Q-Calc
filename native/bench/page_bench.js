// Injected into the bench copy of the web-view app's quick.html only (app_bench.py); never part of a build.
// Reports when the page is ready, then takes typing specs from app_bench.py over a long poll on 127.0.0.1 and
// runs them the way native/mac/Sources/App/DevBench.swift does in the native app:
//   keys:    each prefix set on the bar's input (value + an input event, as typing does), timed until React has
//            rendered it (the live answer is computed in render) and the event loop has turned once more
//   session: a fixed pace without waiting, ↵ at the end of each line if `enter`
;(function () {
  var BASE = 'http://127.0.0.1:__BENCH_PORT__'
  var scriptAt = Date.now()
  function post(path, obj) {
    return fetch(BASE + path, { method: 'POST', body: JSON.stringify(obj) }).catch(function () {})
  }
  function hop() {
    return new Promise(function (resolve) {
      var ch = new MessageChannel()
      ch.port1.onmessage = function () { resolve() }
      ch.port2.postMessage(0)
    })
  }
  // the bar hides on any click elsewhere, and WebKit throttles a hidden page's timers (up to minutes), so the
  // pace comes from the bench script answering after `ms` instead of from setTimeout
  function sleep(ms) {
    return fetch(BASE + '/sleep?ms=' + ms).catch(function () {})
  }
  function field() { return document.querySelector('.quick-plain') }
  var setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set
  function setText(text) {
    var el = field()
    if (!el) return
    setter.call(el, text)
    el.dispatchEvent(new Event('input', { bubbles: true }))
  }
  function enter() {
    var el = field()
    var o = { key: 'Enter', bubbles: true, cancelable: true }
    el.dispatchEvent(new KeyboardEvent('keydown', o))
    el.dispatchEvent(new KeyboardEvent('keyup', o))
  }
  async function settle() {
    await Promise.resolve()
    await hop()
  }
  async function run(spec) {
    var ms = [], idx = [], started = performance.now()
    setText('')
    await settle()
    for (var i = 0; i < spec.inputs.length; i++) {
      var chars = Array.from(spec.inputs[i])
      var steps = spec.whole ? [spec.inputs[i]] : chars.map(function (_, k) { return chars.slice(0, k + 1).join('') })
      if (spec.mode === 'session') {
        for (var k = 0; k < steps.length - 1; k++) {
          setText(steps[k])
          await sleep(spec.delayMs || 30)
        }
        setText(steps[steps.length - 1])
        await settle()
        if (spec.enter) enter()
        await sleep(300)
        if (spec.clear ?? !spec.enter) setText('')
      } else {
        for (var j = 0; j < steps.length; j++) {
          var t0 = performance.now()
          setText(steps[j])
          await settle()
          ms.push(performance.now() - t0)
          idx.push(i)
        }
        setText('')
        await settle()
      }
    }
    return { ms: ms, idx: idx, timeouts: 0, wallMs: performance.now() - started }
  }
  async function loop() {
    for (;;) {
      var cmd
      try {
        cmd = await (await fetch(BASE + '/cmd')).json()
      } catch (e) {
        await sleep(1000)
        continue
      }
      if (!cmd || cmd.noop) continue
      var result
      try { result = await run(cmd) } catch (e) { result = { error: String(e) } }
      await post('/result', result)
    }
  }
  // ready: the bar's input is mounted, then the app's own engine warm-up (a setTimeout(0) its mount effect queues)
  // has run: one hop lets React's passive effects queue it, the timeout runs after it
  var seen = false
  function check() {
    if (seen || !field()) return
    seen = true
    observer.disconnect()
    var mountedAt = Date.now()
    hop().then(function () {
      setTimeout(function () {
        post('/ready', { scriptAt: scriptAt, mountedAt: mountedAt, warmAt: Date.now() }).then(loop)
      }, 0)
    })
  }
  var observer = new MutationObserver(check)
  observer.observe(document, { childList: true, subtree: true })
})()
