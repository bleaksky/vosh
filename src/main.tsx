import ReactDOM from 'react-dom/client';
import App from './App';
import { SettingsApp } from './SettingsApp';
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

// One frontend bundle, multiple windows: the main window loads App;
// auxiliary Tauri windows pass a `?view=...` query so this entry
// renders the right component for each. StrictMode is off because
// xterm.js does not survive the double-mount dance.
const params = new URLSearchParams(window.location.search);
const view = params.get('view');
const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);
root.render(view === 'settings' ? <SettingsApp /> : <App />);
