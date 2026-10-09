// First, before any other module runs: paint the cached theme, so the
// window never shows the dark stylesheet defaults under a light theme.
import './prepaint';
import ReactDOM from 'react-dom/client';
import MainWindow from './shell/MainWindow';
import { SettingsWindow } from './settings/SettingsWindow';
import { HelpWindow } from './help/HelpWindow';
import { SnoopWindow } from './shell/SnoopWindow';
import { crashNotice } from './shell/crashNotice';
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
// silently. Once the app has mounted the notice keeps to a corner clear
// of the command line, counts the errors that follow, and closes
// (src/shell/crashNotice.ts).
const crash = crashNotice(document);
window.addEventListener('error', (e) => crash.show('uncaught error', e.error ?? e.message));
window.addEventListener('unhandledrejection', (e) => crash.show('unhandled rejection', e.reason));

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
  crash.show('render failed', e);
  throw e;
}
