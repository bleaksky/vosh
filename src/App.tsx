import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Terminal, type TerminalHandle } from './components/Terminal';
import { Input, type InputHandle } from './components/Input';
import { Connect, type ConnectionStatus } from './components/Connect';
import { SidePanel } from './components/SidePanel';
import { AffectsBar } from './components/AffectsBar';
import { Resizable } from './components/Resizable';
import { PRESETS, presetTriggers } from './lib/presets';
import { dockLayoutStore } from './lib/docking';
import {
  checkForUpdate,
  dockLayoutGet,
  getUiConfig,
  onState,
  presetsInstall,
  type DockEntryPersist,
  type StatePayload,
} from './lib/session';
import type { SnapZone } from './lib/docking';
import { StatusBar } from './components/StatusBar';
import { applyTheme } from './lib/theme';

const SIDE_PANEL_STORAGE_KEY = 'mudclient.layout.sidePanelOpen';
const DOCK_MIGRATION_KEY = 'mudclient.docking.migrated_phase1_settings_hub';

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
  const [sidePanelOpen, setSidePanelOpen] = useState(loadSidePanelOpen);
  const [fontFamily, setFontFamily] = useState(DEFAULT_FONT_FAMILY);
  const [fontSize, setFontSize] = useState(14);
  const termRef = useRef<TerminalHandle | null>(null);
  const inputRef = useRef<InputHandle | null>(null);

  // One-time migration: drag-to-dock has been removed from the main
  // window so finicky leftover dock state from prior sessions would
  // keep panels misplaced with no way to drag them back. Wipe the
  // dock store the first time this build runs, then mark the
  // migration so it doesn't fire again.
  useEffect(() => {
    try {
      if (!localStorage.getItem(DOCK_MIGRATION_KEY)) {
        dockLayoutStore.reset();
        localStorage.setItem(DOCK_MIGRATION_KEY, '1');
      }
    } catch {
      // ignore storage failures
    }
  }, []);

  // Pull the persisted dock layout from the backend on startup, then
  // listen for cross-window broadcasts so edits made in the Layout
  // Editor window apply here without a relaunch.
  useEffect(() => {
    const isValidZone = (z: string): z is SnapZone =>
      ['top-left', 'top', 'top-right', 'left', 'right', 'bottom-left', 'bottom', 'bottom-right'].includes(z);
    const apply = (entries: DockEntryPersist[]) => {
      const valid = entries
        .filter((e) => typeof e.id === 'string' && isValidZone(e.zone))
        .map((e) => ({ id: e.id, zone: e.zone as SnapZone }));
      dockLayoutStore.applyExternal(valid);
    };
    dockLayoutGet().then(apply).catch((e) => console.error('dock_layout_get failed', e));
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<DockEntryPersist[]>('mudclient://dock-layout-changed', (event) => {
      apply(event.payload);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Click anywhere in the middle area focuses the input. Skip if the user
  // is in the middle of selecting text (so they can still copy from the
  // terminal pane), or if they clicked an actual interactive element.
  const handleMiddleMouseDown = (event: MouseEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    if (target.closest('button, input, textarea, select, .drawer')) return;
    const selection = window.getSelection?.();
    if (selection && selection.toString().length > 0) return;
    inputRef.current?.focus();
  };

  useEffect(() => {
    // Tauri creates the main window with visible=false so the user
    // doesn't see a default-styled white flash. Call show() once the
    // theme + font have applied so the window appears already-painted.
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
    // Backstop: even if every promise rejects, reveal after a short
    // timeout so the window never stays hidden indefinitely.
    const fallback = window.setTimeout(reveal, 500);
    const onUnmount = () => window.clearTimeout(fallback);
    // Pick up the persisted theme + font on first render. If the backend
    // isn't ready (early dev mode), fall back to whatever the OS suggests.
    getUiConfig()
      .then(async (cfg) => {
        applyTheme(cfg.theme);
        setFontFamily(cfg.font_family || DEFAULT_FONT_FAMILY);
        setFontSize(cfg.font_size || 14);
        if (cfg.auto_update) {
          checkForUpdate().catch(() => undefined);
        }
        // Re-install enabled highlight presets. Triggers don't survive a
        // restart, so each launch needs to push the bundles back into
        // the engine.
        const enabled = new Set(cfg.enabled_presets);
        const toInstall = PRESETS.filter((p) => enabled.has(p.id)).flatMap(
          presetTriggers,
        );
        if (toInstall.length > 0) {
          try {
            const installed = await presetsInstall(toInstall);
            console.log(
              `[highlights] startup install: ${installed} preset triggers from ${enabled.size} preset(s)`,
            );
          } catch (e) {
            console.error(
              '[highlights] startup install FAILED — open Settings → Highlights and click "reset to defaults":',
              e,
            );
          }
        }
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
    window.addEventListener('mudclient:font-changed', handler as EventListener);
    return () =>
      window.removeEventListener('mudclient:font-changed', handler as EventListener);
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

  const openSettingsWindow = () => {
    invoke('open_settings_window').catch((e) => {
      handleError(`failed to open settings window: ${String(e)}`);
    });
  };

  return (
    <main className="app">
      <Connect
        status={status}
        onError={handleError}
        onToggleSidePanel={() => setSidePanelOpen((v) => !v)}
        sidePanelOpen={sidePanelOpen}
        onOpenSettings={openSettingsWindow}
      />
      <div className="middle" onMouseUp={handleMiddleMouseDown}>
        <div className="terminal-column">
          <Terminal
            fontFamily={fontFamily}
            fontSize={fontSize}
            onReady={(handle) => {
              termRef.current = handle;
            }}
          />
        </div>
        {sidePanelOpen && (
          <Resizable
            storageKey="mudclient.layout.sidePanelWidth"
            defaultWidth={320}
            minWidth={220}
            maxWidth={1400}
            handleLabel="resize side panel"
          >
            <SidePanel />
          </Resizable>
        )}
      </div>
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
