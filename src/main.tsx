import ReactDOM from 'react-dom/client';
import App from './App';
import './styles.css';

// StrictMode double-mounts effects in dev to surface side-effect bugs.
// xterm.js doesn't survive the dance well, so we render without it.
const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);
root.render(<App />);
