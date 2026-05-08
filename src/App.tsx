import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { type TerminalHandle } from './components/Terminal';
import { Input, type InputHandle } from './components/Input';
import { Connect, type ConnectionStatus } from './components/Connect';
import { TriggersDrawer } from './components/TriggersDrawer';
import { SettingsDrawer } from './components/SettingsDrawer';
import { AffectsBar } from './components/AffectsBar';
import { DockableMiddle } from './components/DockableMiddle';
import { Resizable } from './components/Resizable';
import { SearchView } from './components/SearchView';
import { StatusBar } from './components/StatusBar';
import {
  checkForUpdate,
  getUiConfig,
  onState,
  setDockLayout,
  type StatePayload,
} from './lib/session';
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
  const [dockLayout, setDockLayoutState] = useState('');
  const termRef = useRef<TerminalHandle | null>(null);
  const inputRef = useRef<InputHandle | null>(null);
  const dockSaveTimer = useRef<number | null>(null);

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
        setDockLayoutState(cfg.dock_layout ?? '');
        if (cfg.auto_update) {
          // Background check; failures are silent so a missing endpoint
          // doesn't pop an error banner on every launch.
          checkForUpdate().catch(() => undefined);
        }
      })
      .catch(() => applyTheme('system'));
  }, []);

  // Debounced layout persistence — dockview emits move/add/remove
  // events frequently during a drag and we don't want to burn the
  // backend writing on each one.
  const handleDockLayoutChange = (json: string) => {
    if (dockSaveTimer.current !== null) {
      window.clearTimeout(dockSaveTimer.current);
    }
    dockSaveTimer.current = window.setTimeout(() => {
      dockSaveTimer.current = null;
      setDockLayout(json).catch(() => undefined);
    }, 400);
  };

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
    // Tauri's listener attach is async, but React StrictMode in dev runs
    // the effect cleanup before the promise resolves. If we just stash
    // unsub when it lands, the first listener leaks and the second mount
    // adds another, so disconnect events fire twice. Track a cancelled
    // flag so a late-arriving handle gets unlistened immediately.
    let unsub: (() => void) | undefined;
    let cancelled = false;
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
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
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
        <DockableMiddle
          fontFamily={fontFamily}
          fontSize={fontSize}
          onTerminalReady={(handle) => {
            termRef.current = handle;
          }}
          savedLayout={dockLayout}
          onLayoutChange={handleDockLayoutChange}
        />
        {searchOpen && (
          <Resizable
            storageKey="mudclient.layout.searchWidth"
            defaultWidth={420}
            minWidth={280}
            maxWidth={900}
            handleLabel="resize search panel"
          >
            <SearchView onError={handleError} />
          </Resizable>
        )}
        {triggersOpen && (
          <Resizable
            storageKey="mudclient.layout.triggersWidth"
            defaultWidth={560}
            minWidth={320}
            maxWidth={1100}
            handleLabel="resize triggers drawer"
          >
            <TriggersDrawer
              open={triggersOpen}
              onClose={() => setTriggersOpen(false)}
              onError={handleError}
            />
          </Resizable>
        )}
        {settingsOpen && (
          <Resizable
            storageKey="mudclient.layout.settingsWidth"
            defaultWidth={560}
            minWidth={320}
            maxWidth={1100}
            handleLabel="resize settings drawer"
          >
            <SettingsDrawer
              open={settingsOpen}
              onClose={() => setSettingsOpen(false)}
              onError={handleError}
            />
          </Resizable>
        )}
      </div>
      {/* Bottom rail. flex: 0 0 auto in the .app flex column, so the
          terminal area above (.middle, flex: 1 1 0) takes whatever
          space remains and the rail anchors to the bottom of the
          viewport without any positioning trickery. */}
      <div className="bottom-rail" aria-label="bottom rail">
        <Input
          ref={inputRef}
          enabled
          onError={handleError}
          onLocalEcho={(text) => termRef.current?.write(text)}
        />
        <StatusBar />
        <AffectsBar />
      </div>
    </main>
  );
}

export default App;
