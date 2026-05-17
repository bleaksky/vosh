import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Terminal, type TerminalHandle } from './components/Terminal';
import { Input, type InputHandle } from './components/Input';
import { Connect, type ConnectionStatus } from './components/Connect';
import { getUiConfig, onState, type StatePayload } from './lib/session';
import { applyTheme } from './lib/theme';

const RENAME_MIGRATION_KEY = 'vosh.migration.from_mudclient';

// One-shot rename migration: when the project was renamed from
// "mudclient" to "vosh" the localStorage namespace changed too. On
// first run after the rename, copy every `mudclient.*` key to its
// `vosh.*` counterpart (only if the new key doesn't already exist)
// and delete the originals.
function migrateMudclientKeys(): void {
  try {
    if (localStorage.getItem(RENAME_MIGRATION_KEY)) return;
    const toMove: [string, string][] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (!key || !key.startsWith('mudclient.')) continue;
      const newKey = `vosh.${key.slice('mudclient.'.length)}`;
      toMove.push([key, newKey]);
    }
    for (const [oldKey, newKey] of toMove) {
      const value = localStorage.getItem(oldKey);
      if (value === null) continue;
      if (localStorage.getItem(newKey) === null) {
        localStorage.setItem(newKey, value);
      }
      localStorage.removeItem(oldKey);
    }
    localStorage.setItem(RENAME_MIGRATION_KEY, '1');
  } catch {
    // ignore storage failures (private mode, quota)
  }
}

migrateMudclientKeys();

const DEFAULT_FONT_FAMILY =
  'BerkeleyMono Nerd Font, JetBrains Mono, Fira Code, Menlo, Consolas, ui-monospace, monospace';

function App() {
  const [status, setStatus] = useState<ConnectionStatus>({ kind: 'idle' });
  const [fontFamily, setFontFamily] = useState(DEFAULT_FONT_FAMILY);
  const [fontSize, setFontSize] = useState(14);
  const termRef = useRef<TerminalHandle | null>(null);
  const inputRef = useRef<InputHandle | null>(null);

  // Click anywhere in the terminal area focuses the input. Skip when
  // the user is selecting text (so copy still works) or clicking an
  // actual interactive element.
  const handleTerminalMouseUp = (event: MouseEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    if (target.closest('button, input, textarea, select')) return;
    const selection = window.getSelection?.();
    if (selection && selection.toString().length > 0) return;
    inputRef.current?.focus();
  };

  useEffect(() => {
    // Tauri creates the main window with visible=false so the user
    // doesn't see a default-styled white flash. Reveal once theme and
    // font have applied.
    let revealed = false;
    const reveal = () => {
      if (revealed) return;
      revealed = true;
      const win = getCurrentWindow();
      win
        .show()
        .then(() => win.setFocus())
        .catch((e) => console.error('[main] window show failed', e));
    };
    const fallback = window.setTimeout(reveal, 500);
    const onUnmount = () => window.clearTimeout(fallback);
    getUiConfig()
      .then((cfg) => {
        applyTheme(cfg.theme);
        setFontFamily(cfg.font_family || DEFAULT_FONT_FAMILY);
        setFontSize(cfg.font_size || 14);
      })
      .catch(() => applyTheme('system'))
      .finally(reveal);
    return onUnmount;
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty('--app-font-family', fontFamily);
    root.style.setProperty('--app-font-size', `${fontSize}px`);
  }, [fontFamily, fontSize]);

  useEffect(() => {
    const handler = (event: Event) => {
      const detail = (event as CustomEvent<{ family: string; size: number }>).detail;
      setFontFamily(detail.family || DEFAULT_FONT_FAMILY);
      setFontSize(detail.size || 14);
    };
    window.addEventListener('vosh:font-changed', handler as EventListener);
    return () =>
      window.removeEventListener('vosh:font-changed', handler as EventListener);
  }, []);

  useEffect(() => {
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

  const handleError = (message: string) => {
    setStatus({ kind: 'error', message });
    termRef.current?.write(`\r\n\x1b[31m[${message}]\x1b[0m\r\n`);
  };

  const connected = status.kind === 'connected' || status.kind === 'connecting';

  return (
    <main className="app">
      <Connect status={status} onError={handleError} />
      <div className="terminal-area" onMouseUp={handleTerminalMouseUp}>
        <Terminal
          fontFamily={fontFamily}
          fontSize={fontSize}
          onReady={(handle) => {
            termRef.current = handle;
          }}
        />
      </div>
      <Input
        ref={inputRef}
        enabled={connected}
        onError={handleError}
        onLocalEcho={(text) => termRef.current?.write(text)}
      />
    </main>
  );
}

export default App;
