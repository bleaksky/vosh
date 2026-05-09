import ReactDOM from 'react-dom/client';
import App from './App';
import { SettingsApp } from './SettingsApp';
import './styles.css';
import './styles.docking.css';

// StrictMode double-mounts effects in dev to surface side-effect bugs.
// xterm.js doesn't survive the dance well — the canvas the first
// mount opens lingers in some Tauri builds and visually overlaps the
// second mount's output. Run without StrictMode for now; the lifecycle
// bugs we'd catch with it are easier to spot in production builds
// anyway.
//
// One frontend bundle, two roots: the main window loads the App as
// usual; the standalone Settings window is opened with the URL
// `index.html?view=settings` from the Rust backend, and we branch
// here on that query so the same React entry point can render either.
const params = new URLSearchParams(window.location.search);
const view = params.get('view');
const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);
root.render(view === 'settings' ? <SettingsApp /> : <App />);
