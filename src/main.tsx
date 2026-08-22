import ReactDOM from 'react-dom/client';
import App from './App';
import { SettingsApp } from './SettingsApp';
// Chrome typefaces for the Ember redesign. Bundled through Vite so the
// app never fetches fonts at runtime. Inter carries chrome body text,
// Rajdhani the uppercase pane labels, Roboto Slab the wordmark and
// window titles. Terminal text stays on the bundled mono faces.
import '@fontsource-variable/inter';
import '@fontsource/rajdhani/500.css';
import '@fontsource/rajdhani/600.css';
import '@fontsource/rajdhani/700.css';
import '@fontsource-variable/roboto-slab';
import './styles.css';

// Tag the document with the host OS so CSS can apply per-platform
// tweaks. The two known cases that matter today:
//   - Windows: the frameless-transparent Tauri window cannot composite
//     behind rounded corners, so `border-radius` on `.app` leaks white
//     at the corners. CSS drops the radius when this attribute is
//     `windows`.
//   - Windows + Linux: WebView2 / WebKitGTK use the system scrollbar
//     gutter (chunky white on Windows). Global `::-webkit-scrollbar`
//     theming covers all three platforms; the attribute is only
//     consulted for the radius case so far, but future per-platform
//     adjustments hook here too.
// Detection uses the user agent — `navigator.userAgentData.platform`
// is the modern API but only ships on Chromium >= 90; userAgent
// works everywhere and the heuristic doesn't need to be perfect.
const ua = navigator.userAgent;
const platform = /Win(dows|64|32|NT)/.test(ua)
  ? 'windows'
  : /Mac OS X/.test(ua)
    ? 'macos'
    : /Linux|X11/.test(ua)
      ? 'linux'
      : 'unknown';
document.documentElement.dataset.platform = platform;

// Crash trap. The webview occasionally comes back from a reload as a
// bare themed background with no chrome at all, and WKWebView gives
// no console to read. Any uncaught error or rejection paints itself
// into the page so the failure names itself instead of wedging
// silently.
function showBootError(label: string, detail: unknown) {
  try {
    const el = document.createElement('pre');
    el.style.cssText =
      'position:fixed;left:12px;bottom:12px;right:12px;z-index:99999;max-height:40vh;' +
      'overflow:auto;background:#3a1215;color:#f0b0a8;border:1px solid #7a2a28;' +
      'border-radius:8px;padding:10px 14px;font:11px ui-monospace,monospace;white-space:pre-wrap;';
    const err =
      detail instanceof Error ? `${detail.message}\n${detail.stack ?? ''}` : String(detail);
    el.textContent = `${label}: ${err}`;
    document.body.appendChild(el);
  } catch {
    // the trap must never throw
  }
}
window.addEventListener('error', (e) => showBootError('uncaught error', e.error ?? e.message));
window.addEventListener('unhandledrejection', (e) =>
  showBootError('unhandled rejection', e.reason),
);

// TEMPORARY dev diagnostic for the blank-chrome wedge: the webview
// keeps executing JS while painting nothing, so the page reports its
// state to the vite dev server every few seconds and errors land in
// the dev log. Remove once the wedge is diagnosed.
let trapErrCount = 0;
let lastTrapMsg = '';
window.addEventListener('error', (e) => {
  trapErrCount += 1;
  lastTrapMsg = String(e.message ?? e.error).slice(0, 200);
});
window.addEventListener('unhandledrejection', (e) => {
  trapErrCount += 1;
  lastTrapMsg = String(e.reason).slice(0, 200);
});
if (import.meta.env.DEV) {
  let beat = 0;
  window.setInterval(() => {
    try {
      const rootEl = document.getElementById('root');
      const app = document.querySelector('main.app, .settings-app');
      const r = app?.getBoundingClientRect();
      beat += 1;
      // Enumerate every element whose box covers most of the viewport
      // and paints a non-transparent background. elementFromPoint
      // skips pointer-events:none nodes, so a pass-through overlay
      // hides from point probes — this sweep catches it anyway.
      const covers: string[] = [];
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      document.querySelectorAll('body *').forEach((el) => {
        const b = el.getBoundingClientRect();
        if (b.width < vw * 0.9 || b.height < vh * 0.9) return;
        const cs = getComputedStyle(el);
        const bg = cs.backgroundColor;
        if (bg === 'rgba(0, 0, 0, 0)' || bg === 'transparent') return;
        covers.push(
          `${el.tagName.toLowerCase()}.${String(el.className).slice(0, 40)} bg=${bg} z=${cs.zIndex} pos=${cs.position} pe=${cs.pointerEvents}`,
        );
      });
      const probe = (x: number, y: number) => {
        const el = document.elementFromPoint(x, y);
        return el ? `${el.tagName.toLowerCase()}.${String(el.className).slice(0, 60)}` : 'none';
      };
      void fetch('/__vosh-dbg', {
        method: 'POST',
        body: JSON.stringify({
          beat,
          kids: rootEl?.childElementCount ?? -1,
          app: r ? `${Math.round(r.width)}x${Math.round(r.height)}` : 'none',
          covers,
          top: probe(640, 18),
          rail: probe((r?.width ?? 1200) - 100, 400),
          input: probe(640, (r?.height ?? 900) - 40),
          vis: document.visibilityState,
          err: trapErrCount,
          lastErr: lastTrapMsg,
        }),
      }).catch(() => {});
    } catch {
      // diagnostic only
    }
  }, 5000);
}

// One frontend bundle, multiple windows: the main window loads App;
// auxiliary Tauri windows pass a `?view=...` query so this entry
// renders the right component for each. StrictMode is off because
// xterm.js does not survive the double-mount dance.
const params = new URLSearchParams(window.location.search);
const view = params.get('view');
const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);
try {
  root.render(view === 'settings' ? <SettingsApp /> : <App />);
} catch (e) {
  showBootError('render failed', e);
  throw e;
}
