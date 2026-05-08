import ReactDOM from 'react-dom/client';
import App from './App';
import './styles.css';

// StrictMode double-mounts effects in dev to surface side-effect bugs.
// xterm.js doesn't survive the dance well — the canvas the first
// mount opens lingers in some Tauri builds and visually overlaps the
// second mount's output. Run without StrictMode for now; the lifecycle
// bugs we'd catch with it are easier to spot in production builds
// anyway.
ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(<App />);
