import ReactDOM from 'react-dom/client';
import App from './App';
import { SettingsApp } from './SettingsApp';
import './styles.css';

// One frontend bundle, multiple windows: the main window loads App;
// auxiliary Tauri windows pass a `?view=...` query so this entry
// renders the right component for each. StrictMode is off because
// xterm.js does not survive the double-mount dance.
const params = new URLSearchParams(window.location.search);
const view = params.get('view');
const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);
root.render(view === 'settings' ? <SettingsApp /> : <App />);
