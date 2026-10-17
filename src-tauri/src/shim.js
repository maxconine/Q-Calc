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
  // the usage counts the host kept (null before the first save); only the overlay reports
  window.__QCALC_ANALYTICS = boot.analytics
  // "Windows 11 26200", sent along with them
  if (boot.os) window.__QCALC_OS = boot.os
  // the window can grow past the bar's sides for the 420 smoke
  window.__QCALC_SMOKE_ROOM = true
  window.__QCALC_KEYS = []
  window.__qcalcNativeResult = window.__qcalcNativeResult || function (reply) {
    window.dispatchEvent(new CustomEvent('qcalc-soulver', { detail: reply }))
  }
  withRoot(function (root) {
    root.classList.add('quick-native')
    root.style.overflow = 'hidden'
    // the page keeps the bar this wide inside a window the 420 smoke has grown; rust sets it again on each show
    // and after a side edge's drag
    if (boot.width && root.style.setProperty) root.style.setProperty('--bar-width', boot.width + 'px')
  })

  window.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') {
      // an open game takes esc itself (full screen, its lobby, pause); the page closes it from the lobby
      if (window.__QCALC_GAME_OPEN) return
      e.preventDefault()
      e.stopPropagation()
      // a layer the page has open steps back first, as on the mac's escapeLayer; only then does esc hide the bar
      if (window.__qcalcEscape && window.__qcalcEscape()) return
      send({ type: 'dismiss' })
      return
    }
    // ctrl+w closes the bar like any window, whatever the page has open. a reference sheet (identities, calculus,
    // the periodic table) goes first, as ⌘W closes the mac's sheet windows before the bar
    if ((e.key || '').toLowerCase() === 'w' && e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
      e.preventDefault()
      e.stopPropagation()
      if (window.__qcalcCloseSheet && window.__qcalcCloseSheet()) return
      send({ type: 'dismiss' })
      return
    }
    var field = document.querySelector('.quick-plain')
    if (field && document.activeElement !== field && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      window.__QCALC_KEYS.push(e.key)
    }
  }, true)

  // the mac's edges, which its app takes before the page sees the press; here the input covers the bar and would
  // take it instead, so these listeners go first. along the top (and down the top corners) and a thin strip along
  // the bottom a press moves the bar and a double click forgets the remembered spot; down either side, a drag
  // makes the bar wider or narrower
  var EDGE = 14
  var BOTTOM = 5
  var GRIP = 8
  // the bar inside the window: the same thing, until the 420 smoke opens its room around it
  function bar() {
    var el = document.querySelector('.spotlight')
    var box = el && el.getBoundingClientRect()
    return box && box.width ? box : { left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight }
  }
  // 'move', 'left', 'right', or '' where the press is the page's
  function zone(e) {
    var t = e.target
    if (t && t.closest && t.closest('button, .tape')) return ''
    // nothing to move or widen while a game covers the display
    if (document.fullscreenElement) return ''
    var box = bar()
    var x = e.clientX
    var y = e.clientY
    if (x < box.left || x > box.right || y < box.top || y > box.bottom) return ''
    if (y <= box.top + EDGE) return 'move'
    if (y <= box.top + 44 && (x <= box.left + EDGE || x >= box.right - EDGE)) return 'move'
    if (y >= box.bottom - BOTTOM) return 'move'
    // with the smoke's room open the window is wider than the bar, and it's gone in a moment
    if (box.right - box.left < window.innerWidth - 1) return ''
    if (x <= box.left + GRIP) return 'left'
    if (x >= box.right - GRIP) return 'right'
    return ''
  }
  // the pointer says what a press would do; the input's own text cursor would hide that along its edges
  var cursors = { move: 'move', left: 'ew-resize', right: 'ew-resize' }
  var shownCursor = ''
  function showCursor(next) {
    if (next === shownCursor) return
    shownCursor = next
    withRoot(function (root) {
      if (next) root.dataset.grip = next
      else delete root.dataset.grip
    })
  }
  withRoot(function () {
    if (!document.createElement || !document.head) return
    var style = document.createElement('style')
    style.textContent =
      'html[data-grip="move"], html[data-grip="move"] * { cursor: move !important }' +
      'html[data-grip="ew-resize"], html[data-grip="ew-resize"] * { cursor: ew-resize !important }'
    document.head.appendChild(style)
  })
  var widening = ''
  window.addEventListener('mousemove', function (e) {
    if (!widening) showCursor(cursors[zone(e)] || '')
  }, true)
  window.addEventListener('mouseleave', function () {
    if (!widening) showCursor('')
  }, true)

  // screen pixels, so the window moving under the pointer (a left edge's drag) doesn't feed back into the drag;
  // one message a frame at most
  function widen(side, startX) {
    var last = startX
    var owed = 0
    var queued = false
    widening = side
    // the bar is the whole window while its edge is dragged, so it follows the window with no gap; rust sets the
    // settled width when the drag ends
    withRoot(function (root) {
      if (root.style.setProperty) root.style.setProperty('--bar-width', '100vw')
    })
    function flush() {
      queued = false
      if (!owed) return
      send({ type: 'widen', side: side, dx: owed })
      owed = 0
    }
    function move(e) {
      var dx = e.screenX - last
      last = e.screenX
      owed += side === 'right' ? dx : -dx
      if (queued) return
      queued = true
      if (window.requestAnimationFrame) window.requestAnimationFrame(flush)
      else flush()
    }
    function up() {
      window.removeEventListener('mousemove', move, true)
      window.removeEventListener('mouseup', up, true)
      flush()
      widening = ''
      send({ type: 'widenEnd', side: side })
    }
    window.addEventListener('mousemove', move, true)
    window.addEventListener('mouseup', up, true)
  }

  window.addEventListener('mousedown', function (e) {
    if (e.button !== 0) return
    var where = zone(e)
    if (!where) return
    e.preventDefault()
    e.stopPropagation()
    if (where === 'move') send({ type: e.detail === 2 ? 'recenter' : 'drag' })
    else widen(where, e.screenX)
  }, true)
  window.addEventListener('wheel', function (e) {
    var t = e.target
    if (t && t.closest && t.closest('.tape')) return
    e.preventDefault()
  }, { passive: false, capture: true })
})(__QCALC_BOOT__)
