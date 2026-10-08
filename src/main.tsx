// First, before any other module runs: paint the cached theme, so the
// window never shows the dark stylesheet defaults under a light theme.
import './prepaint';
import ReactDOM from 'react-dom/client';
import MainWindow from './shell/MainWindow';
import { SettingsWindow } from './settings/SettingsWindow';
import { HelpWindow } from './help/HelpWindow';
import { SnoopWindow } from './shell/SnoopWindow';
import './styles/index.css';

// Tag the document with the host OS so CSS can apply per-platform
// tweaks. The two known cases that matter today:
//   - Windows: the frameless-transparent Tauri window cannot composite
//     behind rounded corners, so `border-radius` on `.shell` leaks white
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

// One frontend bundle, multiple windows: the main window loads MainWindow;
// auxiliary Tauri windows pass a `?view=...` query so this entry
// renders the right component for each. A snoop window names its
// session too, `?view=snoop&session=N`. StrictMode is off because
// xterm.js does not survive the double-mount dance.
const params = new URLSearchParams(window.location.search);
const view = params.get('view');
const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);
try {
  root.render(
    view === 'settings' ? (
      <SettingsWindow />
    ) : view === 'help' ? (
      <HelpWindow />
    ) : view === 'snoop' ? (
      <SnoopWindow session={Number(params.get('session'))} />
    ) : (
      <MainWindow />
    ),
  );
} catch (e) {
  showBootError('render failed', e);
  throw e;
}
