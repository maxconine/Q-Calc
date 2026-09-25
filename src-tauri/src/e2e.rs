// the smoke test's look diagnostics: only with QCALC_E2E set, lines land in %TEMP%\qcalc-e2e-look.txt for it to log
use std::io::Write;

pub fn enabled() -> bool {
    std::env::var_os("QCALC_E2E").is_some()
}

pub fn note(line: &str) {
    if !enabled() {
        return;
    }
    let path = std::env::temp_dir().join("qcalc-e2e-look.txt");
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

// the page's side: what it paints under the bar and what chromium renders with (rust asks for this on each show,
// since a hidden web view may never see focus or visibility events), the ctrl keys it sees, and 420's smoke room
pub const PAGE: &str = r#"
;(function () {
  function post(lines) {
    var tauri = window.__TAURI_INTERNALS__;
    if (tauri) tauri.invoke('host', { message: { type: 'e2e', lines: lines } }).catch(function () {});
  }
  var reported = false;
  window.__qcalcE2eReport = function () {
    if (reported) return;
    reported = true;
    var bg = function (sel) { var el = document.querySelector(sel); return el ? getComputedStyle(el).backgroundColor : 'missing' };
    var gl;
    try {
      var c = document.createElement('canvas').getContext('webgl');
      var info = c && c.getExtension('WEBGL_debug_renderer_info');
      gl = !c ? 'none' : c.getParameter(info ? info.UNMASKED_RENDERER_WEBGL : c.RENDERER);
    } catch (e) { gl = 'error ' + e }
    var root = document.documentElement;
    var field = document.querySelector('.quick-plain');
    var frames = 0;
    var tick = function () { frames++; if (frames < 1000) requestAnimationFrame(tick) };
    requestAnimationFrame(tick);
    setTimeout(function () {
      post([
        'page backgrounds: html ' + bg('html') + ', body ' + bg('body') + ', #root ' + bg('#root') + ', .quick-app ' + bg('.quick-app') + ', .spotlight ' + bg('.spotlight'),
        'page: class "' + root.className + '", data-host ' + root.dataset.host + ', theme ' + root.dataset.theme + ', system dark ' + matchMedia('(prefers-color-scheme: dark)').matches + ', reduced motion ' + matchMedia('(prefers-reduced-motion: reduce)').matches + ', hidden ' + document.hidden + ', dpr ' + devicePixelRatio,
        'page font: ' + (field ? getComputedStyle(field).fontFamily : 'no field') + '; ready state ' + document.readyState,
        'webgl renderer: ' + gl + '; animation frames in 500 ms: ' + frames
      ]);
    }, 500);
  };
  addEventListener('keydown', function (e) {
    if (!e.ctrlKey && e.key !== ',') return;
    post(['page keydown key "' + e.key + '" code ' + e.code + ' ctrl ' + e.ctrlKey + ' alt ' + e.altKey + ' shift ' + e.shiftKey + ' prevented ' + e.defaultPrevented + ' target ' + (e.target && e.target.className)]);
  }, true);
  addEventListener('input', function (e) {
    var t = e.target;
    if (!t || t.value !== '420') return;
    setTimeout(function () {
      var room = document.querySelector('.four-twenty-room');
      var live = document.querySelector('.live');
      post(['page input "420": answer "' + (live ? live.textContent : 'none') + '", smoke room "' + (room ? room.className : 'none') + '" 600 ms later']);
    }, 600);
  }, true);
  new MutationObserver(function (records) {
    records.forEach(function (r) {
      var el = r.target;
      if (el.classList && el.classList.contains('four-twenty-room')) {
        post(['smoke room: class "' + el.className + '", ' + el.getBoundingClientRect().height + ' px']);
      }
    });
  }).observe(document, { subtree: true, attributes: true, attributeFilter: ['class'] });
})();
"#;
