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

// what the page itself paints under the bar, and what chromium renders with
pub const PAGE: &str = r#"
addEventListener('load', function () {
  setTimeout(function () {
    var bg = function (sel) { var el = document.querySelector(sel); return el ? getComputedStyle(el).backgroundColor : 'missing' };
    var gl;
    try {
      var c = document.createElement('canvas').getContext('webgl');
      var info = c && c.getExtension('WEBGL_debug_renderer_info');
      gl = !c ? 'none' : c.getParameter(info ? info.UNMASKED_RENDERER_WEBGL : c.RENDERER);
    } catch (e) { gl = 'error ' + e }
    var root = document.documentElement;
    var field = document.querySelector('.quick-plain');
    window.__TAURI_INTERNALS__.invoke('host', { message: { type: 'e2e', lines: [
      'page backgrounds: html ' + bg('html') + ', body ' + bg('body') + ', #root ' + bg('#root') + ', .quick-app ' + bg('.quick-app') + ', .spotlight ' + bg('.spotlight'),
      'page: class "' + root.className + '", data-host ' + root.dataset.host + ', theme ' + root.dataset.theme + ', system dark ' + matchMedia('(prefers-color-scheme: dark)').matches + ', dpr ' + devicePixelRatio,
      'page font: ' + (field ? getComputedStyle(field).fontFamily : 'no field'),
      'webgl renderer: ' + gl
    ] } });
  }, 1500);
});
"#;
