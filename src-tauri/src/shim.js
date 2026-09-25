// the page talks to the mac host through webkit message handlers; this gives it the same door into rust
(function (boot) {
  function send(message) {
    var tauri = window.__TAURI_INTERNALS__
    if (tauri) tauri.invoke('host', { message: message }).catch(function () {})
  }
  var qcalc = { postMessage: send }
  // webview2 runs this before the document has its <html>; touching the root then threw and took the rest of
  // this script with it, so anything on the root waits until the element exists
  function withRoot(fn) {
    if (document.documentElement) return fn(document.documentElement)
    var watch = new MutationObserver(function () {
      if (!document.documentElement) return
      watch.disconnect()
      fn(document.documentElement)
    })
    watch.observe(document, { childList: true })
  }
  // wkwebview (tauri dev on a mac) has a read-only webkit of its own; its other handlers stay reachable
  var own = window.webkit && window.webkit.messageHandlers
  var handlers = own
    ? new Proxy(own, { get: function (target, key) { return key === 'qcalc' ? qcalc : target[key] } })
    : { qcalc: qcalc }
  Object.defineProperty(window, 'webkit', { value: { messageHandlers: handlers }, configurable: true })

  window.__QCALC_PLATFORM = 'windows'
  window.__QCALC_SETTINGS = boot.settings
  window.__QCALC_ONBOARDING = boot.onboarding
  if (boot.rates) window.__QCALC_RATES = boot.rates
  withRoot(function (root) {
    root.dataset.theme = boot.settings.theme
    // windows-only css hangs off this; the mac app and the web page never set it
    root.dataset.host = 'windows'
  })

  // reload or print would throw the page away
  window.addEventListener('keydown', function (e) {
    var key = (e.key || '').toLowerCase()
    if (e.key === 'F5' || (e.ctrlKey && (key === 'r' || key === 'p'))) e.preventDefault()
  }, true)

  if (!boot.overlay) return

  window.__QCALC_NATIVE = true
  window.__QCALC_KEYS = []
  window.__qcalcNativeResult = window.__qcalcNativeResult || function (reply) {
    window.dispatchEvent(new CustomEvent('qcalc-soulver', { detail: reply }))
  }
  withRoot(function (root) {
    root.classList.add('quick-native')
    root.style.overflow = 'hidden'
  })

  window.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      send({ type: 'dismiss' })
      return
    }
    if (e.key === ',' && e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
      e.preventDefault()
      e.stopPropagation()
      send({ type: 'openSettings' })
      return
    }
    var field = document.querySelector('.quick-plain')
    if (field && document.activeElement !== field && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      window.__QCALC_KEYS.push(e.key)
    }
  }, true)
  // the mac's edge strip: a double click along the top forgets the remembered spot
  window.addEventListener('mousedown', function (e) {
    if (e.detail !== 2 || e.button !== 0) return
    var t = e.target
    if (t && t.closest && t.closest('button, .tape')) return
    var edge = 14
    var corner = e.clientY <= 44 && (e.clientX <= edge || e.clientX >= window.innerWidth - edge)
    if (e.clientY > edge && !corner) return
    e.preventDefault()
    e.stopPropagation()
    send({ type: 'recenter' })
  }, true)
  window.addEventListener('wheel', function (e) {
    var t = e.target
    if (t && t.closest && t.closest('.tape')) return
    e.preventDefault()
  }, { passive: false, capture: true })
})(__QCALC_BOOT__)
