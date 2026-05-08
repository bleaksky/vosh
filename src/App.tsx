import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { Terminal, type TerminalHandle } from './components/Terminal';
import { Input, type InputHandle } from './components/Input';
import { Connect, type ConnectionStatus } from './components/Connect';
import { TriggersDrawer } from './components/TriggersDrawer';
import { SettingsDrawer } from './components/SettingsDrawer';
import { SidePanel } from './components/SidePanel';
import { SearchView } from './components/SearchView';
import { StatusBar } from './components/StatusBar';
import { checkForUpdate, getUiConfig, onState, type StatePayload } from './lib/session';
import { applyTheme } from './lib/theme';

const SIDE_PANEL_STORAGE_KEY = 'mudclient.layout.sidePanelOpen';

function loadSidePanelOpen(): boolean {
  try {
    const value = localStorage.getItem(SIDE_PANEL_STORAGE_KEY);
    if (value === null) return true;
    return value === '1';
  } catch {
    return true;
  }
}

const DEFAULT_FONT_FAMILY =
  'BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, Consolas, ui-monospace, monospace';

function App() {
  const [status, setStatus] = useState<ConnectionStatus>({ kind: 'idle' });
  const [triggersOpen, setTriggersOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sidePanelOpen, setSidePanelOpen] = useState(loadSidePanelOpen);
  const [searchOpen, setSearchOpen] = useState(false);
  const [fontFamily, setFontFamily] = useState(DEFAULT_FONT_FAMILY);
  const [fontSize, setFontSize] = useState(14);
  const termRef = useRef<TerminalHandle | null>(null);
  const inputRef = useRef<InputHandle | null>(null);

  // Click anywhere in the middle area focuses the input. Skip if the user
  // is in the middle of selecting text (so they can still copy from the
  // terminal pane), if they clicked an actual interactive element (button,
  // input, textarea), or if the click landed inside the triggers drawer.
  const handleMiddleMouseDown = (event: MouseEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    if (target.closest('button, input, textarea, select, .drawer')) return;
    const selection = window.getSelection?.();
    if (selection && selection.toString().length > 0) return;
    inputRef.current?.focus();
  };

  useEffect(() => {
    // Pick up the persisted theme + font on first render. If the backend
    // isn't ready (early dev mode), fall back to whatever the OS suggests.
    getUiConfig()
      .then((cfg) => {
        applyTheme(cfg.theme);
        setFontFamily(cfg.font_family || DEFAULT_FONT_FAMILY);
        setFontSize(cfg.font_size || 14);
        if (cfg.auto_update) {
          // Background check; failures are silent so a missing endpoint
          // doesn't pop an error banner on every launch.
          checkForUpdate().catch(() => undefined);
        }
      })
      .catch(() => applyTheme('system'));
  }, []);

  useEffect(() => {
    // Mirror the font choice to CSS variables so the chrome (status bar,
    // input row, drawers) shares the same family and size as the
    // terminal pane. Components read the variables in styles.css.
    const root = document.documentElement;
    root.style.setProperty('--app-font-family', fontFamily);
    root.style.setProperty('--app-font-size', `${fontSize}px`);
  }, [fontFamily, fontSize]);

  // Listen for app-wide setting updates emitted by SettingsDrawer so
  // changes apply without a relaunch.
  useEffect(() => {
    const handler = (event: Event) => {
      const detail = (event as CustomEvent<{ family: string; size: number }>).detail;
      setFontFamily(detail.family || DEFAULT_FONT_FAMILY);
      setFontSize(detail.size || 14);
    };
    window.addEventListener('mudclient:font-changed', handler as EventListener);
    return () => window.removeEventListener('mudclient:font-changed', handler as EventListener);
  }, []);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    onState((payload: StatePayload) => {
      if (payload.kind === 'disconnected') {
        setStatus({ kind: 'idle' });
        if (payload.reason && termRef.current) {
          termRef.current.write(`\r\n\x1b[31m[${payload.reason}]\x1b[0m\r\n`);
        }
      } else {
        setStatus(payload);
      }
    }).then((fn) => {
      unsub = fn;
    });
    return () => {
      unsub?.();
    };
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(SIDE_PANEL_STORAGE_KEY, sidePanelOpen ? '1' : '0');
    } catch {
      // ignore storage failures (private mode, quota)
    }
  }, [sidePanelOpen]);

  const handleError = (message: string) => {
    setStatus({ kind: 'error', message });
    termRef.current?.write(`\r\n\x1b[31m[${message}]\x1b[0m\r\n`);
  };

  return (
    <main className="app">
      <Connect
        status={status}
        onError={handleError}
        onToggleTriggers={() => setTriggersOpen((v) => !v)}
        triggersOpen={triggersOpen}
        onToggleSidePanel={() => setSidePanelOpen((v) => !v)}
        sidePanelOpen={sidePanelOpen}
        onToggleSettings={() => setSettingsOpen((v) => !v)}
        settingsOpen={settingsOpen}
        onToggleSearch={() => setSearchOpen((v) => !v)}
        searchOpen={searchOpen}
      />
      <div className="middle" onMouseUp={handleMiddleMouseDown}>
        <Terminal
          fontFamily={fontFamily}
          fontSize={fontSize}
          onReady={(handle) => {
            termRef.current = handle;
          }}
        />
        {searchOpen ? (
          <SearchView onError={handleError} />
        ) : (
          sidePanelOpen && <SidePanel />
        )}
        <TriggersDrawer
          open={triggersOpen}
          onClose={() => setTriggersOpen(false)}
          onError={handleError}
        />
        <SettingsDrawer
          open={settingsOpen}
          onClose={() => setSettingsOpen(false)}
          onError={handleError}
        />
      </div>
      <Input
        ref={inputRef}
        enabled
        onError={handleError}
        onLocalEcho={(text) => termRef.current?.write(text)}
      />
      <StatusBar />
    </main>
  );
}

export default App;
