import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Terminal, type TerminalHandle } from './components/Terminal';
import { Input, type InputHandle } from './components/Input';
import { Connect, type ConnectionStatus } from './components/Connect';
import { TopBar } from './components/TopBar';
import { StatusBar } from './components/StatusBar';
import { Resizable } from './components/Resizable';
import { MapPane } from './components/MapPane';
import { ChatPane } from './components/ChatPane';
import { RoomStrip } from './components/RoomStrip';
import { AffectsBar } from './components/AffectsBar';
import {
  getUiConfig,
  listTriggers,
  onState,
  presetsInstall,
  presetsRemove,
  type StatePayload,
} from './lib/session';
import { applyAndBroadcastTheme } from './lib/theme';
import { loadFontStack } from './lib/fontLoader';
import { defaultEnabledIds, PRESETS, presetTriggers } from './lib/presets';

const MAP_OPEN_KEY = 'vosh.layout.mapOpen';
const CHAT_OPEN_KEY = 'vosh.layout.chatOpen';

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
  '"BerkeleyMono Bundled", "JetBrainsMono Bundled", Menlo, Consolas, ui-monospace, monospace';

function loadFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) === '1';
  } catch {
    return false;
  }
}

function App() {
  const [status, setStatus] = useState<ConnectionStatus>({ kind: 'idle' });
  const [fontFamily, setFontFamily] = useState(DEFAULT_FONT_FAMILY);
  const [fontSize, setFontSize] = useState(14);
  const [mapOpen, setMapOpen] = useState(() => loadFlag(MAP_OPEN_KEY));
  const [chatOpen, setChatOpen] = useState(() => loadFlag(CHAT_OPEN_KEY));
  const termRef = useRef<TerminalHandle | null>(null);
  const inputRef = useRef<InputHandle | null>(null);

  useEffect(() => {
    try {
      localStorage.setItem(MAP_OPEN_KEY, mapOpen ? '1' : '0');
    } catch {
      // ignore storage failures
    }
  }, [mapOpen]);

  useEffect(() => {
    try {
      localStorage.setItem(CHAT_OPEN_KEY, chatOpen ? '1' : '0');
    } catch {
      // ignore storage failures
    }
  }, [chatOpen]);

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
      .then(async (cfg) => {
        void applyAndBroadcastTheme(cfg.theme);
        setFontFamily(cfg.font_family || DEFAULT_FONT_FAMILY);
        setFontSize(cfg.font_size || 14);

        // Sweep orphan preset triggers — anything tagged with a
        // preset id that no longer exists in code (renamed or
        // removed). Triggers persist in profile.toml so without this
        // they'd linger forever after a preset is dropped.
        try {
          const validPresetIds = new Set(PRESETS.map((p) => p.id));
          const allTriggers = await listTriggers();
          const orphanIds = new Set<string>();
          for (const t of allTriggers) {
            if (t.preset && !validPresetIds.has(t.preset)) {
              orphanIds.add(t.preset);
            }
          }
          for (const id of orphanIds) {
            await presetsRemove(id);
          }
        } catch (e) {
          console.error('[presets] orphan sweep failed:', e);
        }

        // Re-install enabled presets so pattern/template changes in
        // the current build overwrite older versions persisted in
        // profile.toml. Defaults to all default-enabled if the user
        // hasn't customized their enabled list yet.
        const enabled = new Set(
          cfg.enabled_presets.length > 0 ? cfg.enabled_presets : defaultEnabledIds(),
        );
        const toInstall = PRESETS.filter((p) => enabled.has(p.id)).flatMap(presetTriggers);
        if (toInstall.length > 0) {
          try {
            await presetsInstall(toInstall);
          } catch (e) {
            console.error('[presets] startup install failed:', e);
          }
        }
      })
      .catch(() => void applyAndBroadcastTheme('system'))
      .finally(reveal);
    return onUnmount;
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    // Inject @font-face blocks for every named family in the stack so
    // WKWebView can render fonts it would otherwise refuse to match.
    loadFontStack(fontFamily);
    root.style.setProperty('--app-font-family', fontFamily);
    root.style.setProperty('--app-font-size', `${fontSize}px`);
  }, [fontFamily, fontSize]);

  useEffect(() => {
    // Cross-window emit from the settings save path. window CustomEvents
    // do not cross webviews, so we listen via the Tauri event bus here.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<{ family: string; size: number }>('vosh://font-changed', (event) => {
      const detail = event.payload;
      setFontFamily(detail.family || DEFAULT_FONT_FAMILY);
      setFontSize(detail.size || 14);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
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
      <TopBar
        mapOpen={mapOpen}
        onToggleMap={() => setMapOpen((v) => !v)}
        chatOpen={chatOpen}
        onToggleChat={() => setChatOpen((v) => !v)}
      />
      <Connect status={status} onError={handleError} />
      <RoomStrip />
      <div className="main-row">
        <div className="terminal-area" onMouseUp={handleTerminalMouseUp}>
          <Terminal
            fontFamily={fontFamily}
            fontSize={fontSize}
            onReady={(handle) => {
              termRef.current = handle;
            }}
          />
        </div>
        {mapOpen && (
          <Resizable
            storageKey="vosh.layout.mapWidth"
            defaultSize={360}
            minSize={240}
            maxSize={720}
            className="map-resizable"
            handleLabel="resize map"
          >
            <MapPane />
          </Resizable>
        )}
      </div>
      {chatOpen && (
        <Resizable
          storageKey="vosh.layout.chatHeight"
          direction="vertical"
          defaultSize={180}
          minSize={100}
          maxSize={500}
          className="chat-resizable"
          handleLabel="resize chat"
        >
          <ChatPane />
        </Resizable>
      )}
      <AffectsBar />
      <Input
        ref={inputRef}
        enabled={connected}
        onError={handleError}
        onLocalEcho={(text) => termRef.current?.write(text)}
      />
      <StatusBar />
    </main>
  );
}

export default App;
