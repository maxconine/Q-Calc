// the page talks to the mac host through webkit message handlers; this gives it the same door into rust
(function () {
  function send(message) {
    var tauri = window.__TAURI_INTERNALS__
    if (tauri) tauri.invoke('host', { message: message }).catch(function () {})
  }
  var qcalc = { postMessage: send }
  // wkwebview (tauri dev on a mac) has a read-only webkit of its own; its other handlers stay reachable
  var own = window.webkit && window.webkit.messageHandlers
  var handlers = own
    ? new Proxy(own, { get: function (target, key) { return key === 'qcalc' ? qcalc : target[key] } })
    : { qcalc: qcalc }
  Object.defineProperty(window, 'webkit', { value: { messageHandlers: handlers }, configurable: true })

  window.__QCALC_NATIVE = true
  window.__QCALC_KEYS = []
  window.__QCALC_SETTINGS = __QCALC_HOTKEY__
  window.__qcalcNativeResult = window.__qcalcNativeResult || function (reply) {
    window.dispatchEvent(new CustomEvent('qcalc-soulver', { detail: reply }))
  }
  document.documentElement.classList.add('quick-native')
  document.documentElement.style.overflow = 'hidden'

  window.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      send({ type: 'dismiss' })
      return
    }
    var field = document.querySelector('.quick-plain')
    if (field && document.activeElement !== field && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      window.__QCALC_KEYS.push(e.key)
    }
  }, true)
  window.addEventListener('wheel', function (e) {
    var t = e.target
    if (t && t.closest && t.closest('.tape')) return
    e.preventDefault()
  }, { passive: false, capture: true })
})()
