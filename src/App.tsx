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
import { GroupPane } from './components/GroupPane';
import { RoomStrip } from './components/RoomStrip';
import { VitalsBar } from './components/VitalsBar';
import { UpdateNotice } from './components/UpdateNotice';
import {
  dockLayoutGet,
  dockLayoutSet,
  getUiConfig,
  listTriggers,
  onState,
  presetsInstall,
  presetsRemove,
  subscribeCustomThemesChanged,
  subscribeDockLayoutChanged,
  subscribeSplitDividerChanged,
  type StatePayload,
} from './lib/session';
import { applyAndBroadcastTheme } from './lib/theme';
import { loadFontStack } from './lib/fontLoader';
import { defaultEnabledIds, PRESETS, presetTriggers } from './lib/presets';
import { customToAppTheme, setCustomThemes } from './lib/themes';
import { startChatStore } from './lib/chatStore';
import { startGroupStore } from './lib/groupStore';
import {
  DEFAULT_PANEL_PLACEMENTS,
  groupPanels,
  PANELS,
  panelPlacementsFromDock,
  panelPlacementsToDock,
  type PanelId,
  type PanelPlacement,
  type Zone,
} from './lib/panels';

const RENAME_MIGRATION_KEY = 'vosh.migration.from_mudclient';

// CSS variable applied to the split-scrollback divider. Empty value
// removes the override so the rule falls back to the theme default.
function applySplitDividerColor(color: string | null): void {
  const root = document.documentElement;
  if (color && color.length > 0) {
    root.style.setProperty('--c-split-divider', color);
  } else {
    root.style.removeProperty('--c-split-divider');
  }
}

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

function App() {
  const [status, setStatus] = useState<ConnectionStatus>({ kind: 'idle' });
  const [fontFamily, setFontFamily] = useState(DEFAULT_FONT_FAMILY);
  const [fontSize, setFontSize] = useState(14);
  const [themeTerminalColors, setThemeTerminalColors] = useState(false);
  // Panel layout. Each panel id maps to a placement: zone + (for
  // left/right zones) vertical alignment. Seeded from the backend
  // dock_layout on mount and kept in sync via the
  // dock-layout-changed broadcast.
  const [panelPlacements, setPanelPlacements] =
    useState<Record<PanelId, PanelPlacement>>(DEFAULT_PANEL_PLACEMENTS);
  const termRef = useRef<TerminalHandle | null>(null);
  const historyTermRef = useRef<TerminalHandle | null>(null);
  const inputRef = useRef<InputHandle | null>(null);
  // Split-scrollback state. When true, a second xterm appears above the
  // live one and shows the same buffer scrolled back so you can read
  // earlier output while live combat keeps streaming below.
  const [splitOpen, setSplitOpen] = useState(false);
  // History pane scroll depth, driven by the Terminal's onScrollPosition
  // callback. Drives the "↑ N / max" indicator in the top-right of the
  // history pane.
  const [historyScrollPos, setHistoryScrollPos] = useState<{
    back: number;
    max: number;
  } | null>(null);

  // Load persisted panel layout once on mount and re-apply when any
  // other window broadcasts a change (Settings → Panels save).
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    dockLayoutGet()
      .then((entries) => {
        if (!cancelled) setPanelPlacements(panelPlacementsFromDock(entries));
      })
      .catch(() => {});
    subscribeDockLayoutChanged((entries) => {
      if (!cancelled) setPanelPlacements(panelPlacementsFromDock(entries));
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Persist a single panel's placement change and broadcast to other
  // windows. The TopBar map/chat toggle buttons go through this so a
  // quick hide/show stays synced with the Settings UI.
  const setPanelZone = (id: PanelId, zone: Zone) => {
    setPanelPlacements((prev) => {
      if (!PANELS[id].allowedZones.includes(zone)) return prev;
      const next: Record<PanelId, PanelPlacement> = {
        ...prev,
        [id]: { ...prev[id], zone },
      };
      void dockLayoutSet(panelPlacementsToDock(next)).catch(() => {});
      return next;
    });
  };

  // Map/chat topbar buttons toggle a panel between hidden and its
  // last-visible zone. Preserves the user's last non-hidden choice
  // via a session-scoped memory so repeated hides + unhides land
  // where the panel was last visible.
  const lastVisibleZoneRef = useRef<Partial<Record<PanelId, Zone>>>({});
  const togglePanelVisibility = (id: PanelId) => {
    const current = panelPlacements[id].zone;
    if (current === 'hidden') {
      const restore = lastVisibleZoneRef.current[id] ?? PANELS[id].defaultZone;
      setPanelZone(id, restore);
    } else {
      lastVisibleZoneRef.current[id] = current;
      setPanelZone(id, 'hidden');
    }
  };

  // Reset the history-pane scroll-depth indicator whenever the split
  // closes. The history Terminal unmounts and the next mount will fire
  // its own onScrollPosition; keeping the prior value here would flash
  // stale numbers for one paint before being overwritten.
  useEffect(() => {
    if (!splitOpen) setHistoryScrollPos(null);
  }, [splitOpen]);

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

  // Bootstrap the chat + group buffers at app launch so any
  // Comm.Channel / routed / Group.Info / Char.Worth pushes that
  // arrive while the chat-group pane is closed (or has not yet
  // been opened) still land in the stores and are visible on
  // first open.
  useEffect(() => {
    startChatStore();
    startGroupStore();
  }, []);

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
        // Register user-authored themes BEFORE the theme apply so
        // the picked theme can actually be a custom entry.
        setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
        void applyAndBroadcastTheme(cfg.theme);
        setFontFamily(cfg.font_family || DEFAULT_FONT_FAMILY);
        setFontSize(cfg.font_size || 14);
        setThemeTerminalColors(cfg.theme_terminal_colors);
        applySplitDividerColor(cfg.split_divider_color);

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
    // Settings save broadcasts the new divider color. Apply it on the
    // main window without a relaunch.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    subscribeSplitDividerChanged((color) => {
      applySplitDividerColor(color);
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
    // Live-flip the terminal palette mode when the user toggles the
    // setting. The Terminal component re-applies the palette on the
    // prop change without recreating xterm.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen<boolean>('vosh://theme-terminal-colors-changed', (event) => {
      setThemeTerminalColors(Boolean(event.payload));
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
    // Custom-themes catalog updates from any other webview. Refreshes
    // the in-memory THEMES registry so a subsequent theme-changed event
    // can find a newly-saved custom theme.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    subscribeCustomThemesChanged((list) => {
      setCustomThemes(list.map(customToAppTheme));
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

  const grouped = groupPanels(panelPlacements);
  const renderPanel = (id: PanelId) => {
    switch (id) {
      case 'map':
        return <MapPane key="map" />;
      case 'group':
        return <GroupPane key="group" pinned onTogglePin={() => setPanelZone('group', 'hidden')} />;
      case 'vitals':
        return <VitalsBar key="vitals" />;
      case 'roomstrip':
        return <RoomStrip key="roomstrip" />;
      case 'chat':
        return <ChatPane key="chat" onClose={() => setPanelZone('chat', 'hidden')} />;
    }
  };

  // Top substack at the top, spacer fills the middle, bottom substack
  // at the bottom. Always render all three so the spacer keeps doing
  // its job when one side is empty — previously the spacer landed
  // after an only-bottom substack, leaving the panel sitting at the
  // top of the column even when the user picked align=bottom.
  const renderSideZoneInner = (top: PanelId[], bottom: PanelId[]) => (
    <div className="panel-zone-stack">
      <div className="panel-zone-substack panel-zone-substack-top">{top.map(renderPanel)}</div>
      <div className="panel-zone-spacer" />
      <div className="panel-zone-substack panel-zone-substack-bottom">
        {bottom.map(renderPanel)}
      </div>
    </div>
  );

  const leftHasAny = grouped.leftTop.length > 0 || grouped.leftBottom.length > 0;
  const rightHasAny = grouped.rightTop.length > 0 || grouped.rightBottom.length > 0;

  return (
    <main className="app">
      <TopBar
        mapOpen={panelPlacements.map.zone !== 'hidden'}
        onToggleMap={() => togglePanelVisibility('map')}
        chatOpen={panelPlacements.chat.zone !== 'hidden'}
        onToggleChat={() => togglePanelVisibility('chat')}
      />
      <Connect status={status} onError={handleError} />
      {grouped.top.length > 0 && (
        <div className="panel-zone panel-zone-top">{grouped.top.map(renderPanel)}</div>
      )}
      <div className="main-row">
        {leftHasAny && (
          <Resizable
            storageKey="vosh.layout.leftZoneWidth"
            anchor="left"
            defaultSize={360}
            minSize={200}
            maxSize={720}
            className="panel-zone panel-zone-left"
            handleLabel="resize left panel zone"
          >
            {renderSideZoneInner(grouped.leftTop, grouped.leftBottom)}
          </Resizable>
        )}
        <div
          className={`terminal-area${splitOpen ? ' terminal-area-split' : ''}`}
          onMouseUp={handleTerminalMouseUp}
        >
          {splitOpen && (
            // Lazy mount. A hidden Terminal cannot be measured by
            // FitAddon (its container is display:none, bounding rect
            // 0x0) so writes wrap at 1-3 columns and the scrollback
            // arrives mangled. Mounting on open guarantees the xterm
            // sizes itself against the visible split layout before
            // any bytes land. The initial scroll runs in
            // onScrollbackLoaded — onReady fires before loadScrollback
            // resolves, and a scrollPages call on an empty terminal
            // is a no-op that the next write would override anyway.
            <div className="terminal-pane terminal-pane-history">
              <Terminal
                fontFamily={fontFamily}
                fontSize={fontSize}
                themeTerminalColors={themeTerminalColors}
                quiet
                onReady={(handle) => {
                  historyTermRef.current = handle;
                }}
                onScrollbackLoaded={() => {
                  // Position the history viewport so its bottom line
                  // sits one row above what the live pane currently
                  // shows at its top. scrollPages(-1) shifts up by
                  // exactly one viewport, which is the history pane
                  // height after the split opens — so the two panes
                  // no longer show overlapping content.
                  historyTermRef.current?.scrollPages(-1);
                }}
                onScrollPosition={(back, max) => setHistoryScrollPos({ back, max })}
              />
              {historyScrollPos && historyScrollPos.max > 0 && (
                <div className="scrollback-indicator" aria-live="polite">
                  ↑ {historyScrollPos.back} / {historyScrollPos.max}
                </div>
              )}
            </div>
          )}
          <div className="terminal-pane terminal-pane-live">
            <Terminal
              fontFamily={fontFamily}
              fontSize={fontSize}
              themeTerminalColors={themeTerminalColors}
              onReady={(handle) => {
                termRef.current = handle;
              }}
            />
          </div>
        </div>
        {rightHasAny && (
          <Resizable
            storageKey="vosh.layout.rightZoneWidth"
            anchor="right"
            defaultSize={360}
            minSize={200}
            maxSize={720}
            className="panel-zone panel-zone-right"
            handleLabel="resize right panel zone"
          >
            {renderSideZoneInner(grouped.rightTop, grouped.rightBottom)}
          </Resizable>
        )}
      </div>
      {grouped.bottom.length > 0 && (
        <div className="panel-zone panel-zone-bottom">{grouped.bottom.map(renderPanel)}</div>
      )}
      <Input
        ref={inputRef}
        enabled={connected}
        onError={handleError}
        onLocalEcho={(text) => termRef.current?.write(text)}
        onScrollTerminal={(pages) => {
          // Split-scrollback gesture. The live pane (termRef) stays
          // anchored to the tail. PageUp opens the split if closed;
          // the history Terminal mounts on that state change and its
          // onReady does the initial scroll, so we don't touch the
          // ref here (it is null until the mount completes).
          if (pages < 0) {
            if (!splitOpen) {
              setSplitOpen(true);
              return;
            }
            historyTermRef.current?.scrollPages(pages);
            return;
          }
          if (!splitOpen) return;
          historyTermRef.current?.scrollPages(pages);
          // After the page-down lands, close the split if we paged
          // all the way back to the live tail.
          queueMicrotask(() => {
            if (historyTermRef.current?.isAtBottom()) setSplitOpen(false);
          });
        }}
        onExitSplit={() => {
          if (splitOpen) setSplitOpen(false);
        }}
      />
      <StatusBar />
      <UpdateNotice />
    </main>
  );
}

export default App;
