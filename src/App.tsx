import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Terminal, type TerminalHandle } from './components/Terminal';
import { Input, type InputHandle } from './components/Input';
import { Connect, type ConnectionStatus } from './components/Connect';
import { TopBar } from './components/TopBar';
import { StatusBar } from './components/StatusBar';
import { Resizable } from './components/Resizable';
import { AffectsBar } from './components/AffectsBar';
import { MapPane } from './components/MapPane';
import { ChatPane } from './components/ChatPane';
import { GroupPane } from './components/GroupPane';
import { RoomStrip } from './components/RoomStrip';
import { VitalsBar } from './components/VitalsBar';
import { UpdateNotice } from './components/UpdateNotice';
import {
  broadcastUiConfigChanges,
  dockLayoutGet,
  dockLayoutSet,
  getUiConfig,
  setWindowSize,
  listTriggers,
  onState,
  presetsInstall,
  presetsRemove,
  subscribeCustomThemesChanged,
  subscribeDockLayoutChanged,
  subscribeProfileSwitched,
  subscribeSidePanelsFillHeightChanged,
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
  DEFAULT_PANEL_LAYOUT,
  groupPanels,
  PANELS,
  panelLayoutFromDock,
  panelLayoutToDock,
  type PanelId,
  type PanelLayout,
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
  const [panelLayout, setPanelLayout] = useState<PanelLayout>(DEFAULT_PANEL_LAYOUT);
  // When true, side panels (left/right zones) extend to the bottom
  // edge of the window and the terminal input + status bar live in
  // a column under the terminal area only. Loaded from UiConfig.
  const [sidePanelsFillHeight, setSidePanelsFillHeight] = useState(false);
  const termRef = useRef<TerminalHandle | null>(null);
  const historyTermRef = useRef<TerminalHandle | null>(null);
  const inputRef = useRef<InputHandle | null>(null);
  // Direct ref on the terminal-area wrapper so we can attach a
  // non-passive wheel listener. JSX onWheel is passive in some
  // React versions and silently no-ops preventDefault, which would
  // let xterm scroll the live pane underneath us.
  const terminalAreaRef = useRef<HTMLDivElement | null>(null);
  // splitOpen state needs to be read inside the wheel handler. The
  // handler is registered once and runs many times, so we mirror the
  // state into a ref to avoid stale closures.
  const splitOpenRef = useRef(false);
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
        if (!cancelled) setPanelLayout(panelLayoutFromDock(entries));
      })
      .catch(() => {});
    subscribeDockLayoutChanged((entries) => {
      if (!cancelled) setPanelLayout(panelLayoutFromDock(entries));
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
    setPanelLayout((prev) => {
      if (!PANELS[id].allowedZones.includes(zone)) return prev;
      const next: PanelLayout = {
        placements: { ...prev.placements, [id]: { ...prev.placements[id], zone } },
        order: prev.order,
      };
      void dockLayoutSet(panelLayoutToDock(next)).catch(() => {});
      return next;
    });
  };

  // Map/chat topbar buttons toggle a panel between hidden and its
  // last-visible zone. Preserves the user's last non-hidden choice
  // via a session-scoped memory so repeated hides + unhides land
  // where the panel was last visible.
  const lastVisibleZoneRef = useRef<Partial<Record<PanelId, Zone>>>({});
  const togglePanelVisibility = (id: PanelId) => {
    const current = panelLayout.placements[id].zone;
    if (current === 'hidden') {
      // Fall back to homeZone (never 'hidden') so the topbar toggle
      // actually shows the panel somewhere even when defaultZone is
      // 'hidden' (e.g., chat is opt-in but lives at the bottom when
      // visible).
      const restore = lastVisibleZoneRef.current[id] ?? PANELS[id].homeZone;
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
    splitOpenRef.current = splitOpen;
  }, [splitOpen]);

  // Whenever the panel layout changes (e.g. chat toggled hidden), the
  // terminal-area's available height shifts. FitAddon's own internal
  // observers do not always pick up the change before xterm draws the
  // next frame, which leaves a stripe of unused padding at the bottom
  // of the terminal until something else (e.g. a scroll) kicks off a
  // refit. Force a fit on every placements change, after layout has
  // settled, so xterm rows match the available height immediately.
  useEffect(() => {
    const id = requestAnimationFrame(() => {
      termRef.current?.fit();
      historyTermRef.current?.fit();
    });
    return () => cancelAnimationFrame(id);
  }, [panelLayout, sidePanelsFillHeight]);

  // Wheel listener attached in capture phase with passive:false so we
  // fire BEFORE the xterm canvas inside terminal-area sees the event.
  // Without capture phase, xterm's own bubble-phase handler scrolls
  // the live pane first and preventDefault is too late; the live pane
  // would scroll along with the history pane any time the cursor hovered
  // over it during a wheel gesture. stopPropagation guarantees the
  // event never reaches xterm at all when we handle it ourselves.
  useEffect(() => {
    const el = terminalAreaRef.current;
    if (!el) return;
    const onWheel = (e: globalThis.WheelEvent) => {
      const dir = Math.sign(e.deltaY);
      if (dir === 0) return;
      const lines = dir * 3;
      if (lines < 0) {
        e.preventDefault();
        e.stopPropagation();
        if (!splitOpenRef.current) {
          setSplitOpen(true);
          return;
        }
        historyTermRef.current?.scrollLines(lines);
        return;
      }
      if (!splitOpenRef.current) return;
      e.preventDefault();
      e.stopPropagation();
      historyTermRef.current?.scrollLines(lines);
      queueMicrotask(() => {
        if (historyTermRef.current?.isAtBottom()) setSplitOpen(false);
      });
    };
    el.addEventListener('wheel', onWheel, { passive: false, capture: true });
    return () => el.removeEventListener('wheel', onWheel, { capture: true });
  }, []);

  // Click anywhere in the terminal area focuses the input. Skip when
  // the user is selecting text (so copy still works) or clicking an
  // actual interactive element.
  const handleTerminalMouseUp = (event: MouseEvent<HTMLDivElement>) => {
    // Middle-click (scroll-wheel click) closes the split-scrollback
    // view and snaps the live pane to the bottom. Standard "remove
    // scrollback break" gesture for users coming from other clients.
    if (event.button === 1) {
      event.preventDefault();
      if (splitOpen) setSplitOpen(false);
      termRef.current?.scrollToBottom();
      return;
    }
    focusInputFromClick(event);
  };

  // Wider click handler attached to the <main>. Catches clicks outside
  // the terminal (panels, chrome) so the user who clicks anywhere in
  // the window — including after pulling focus back from Discord —
  // lands with the command line ready to type.
  const handleAppMouseUp = (event: MouseEvent<HTMLElement>) => {
    focusInputFromClick(event);
  };

  // Shared "click anywhere focuses input" logic. Skips interactive
  // elements (so the actual click handler runs and keeps its own
  // focus state) and selection drags (so copy still works).
  const focusInputFromClick = (event: MouseEvent<Element>) => {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement;
    if (target.closest('button, input, textarea, select, a, [role="button"]')) return;
    const selection = window.getSelection?.();
    if (selection && selection.toString().length > 0) return;
    inputRef.current?.focus();
  };

  // Tauri reports a window-level focus event when the OS brings the
  // app back to front (user clicked the Vosh window while it was
  // unfocused, or alt-tabbed in). Focusing the input here is the
  // "click-to-type" affordance the user expects on every reactivation.
  useEffect(() => {
    const onFocus = () => inputRef.current?.focus();
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);

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
        setSidePanelsFillHeight(cfg.side_panels_fill_height);

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

  // When the active profile changes (manual #profile switch, Settings
  // click, or Char.Status auto-swap after login), the backend has
  // already swapped the in-memory Profile but the frontend's React
  // state still mirrors the old profile's theme / font / vitals /
  // panel layout / etc. Re-fetch the new UiConfig, apply locally, and
  // broadcast the diff so every other window's per-field subscriber
  // settles too. Dock layout sits in its own store and reloads
  // explicitly because its event is not part of the UiConfig fan-out.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void subscribeProfileSwitched(() => {
      void (async () => {
        try {
          const cfg = await getUiConfig();
          if (cancelled) return;
          setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
          void applyAndBroadcastTheme(cfg.theme);
          setFontFamily(cfg.font_family || DEFAULT_FONT_FAMILY);
          setFontSize(cfg.font_size || 14);
          setThemeTerminalColors(cfg.theme_terminal_colors);
          applySplitDividerColor(cfg.split_divider_color);
          setSidePanelsFillHeight(cfg.side_panels_fill_height);
          await broadcastUiConfigChanges(cfg);
          const entries = await dockLayoutGet();
          if (cancelled) return;
          setPanelLayout(panelLayoutFromDock(entries));
        } catch (e) {
          console.error('[app] profile-switched refresh failed', e);
        }
      })();
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
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    subscribeSidePanelsFillHeightChanged((value) => {
      setSidePanelsFillHeight(value);
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
        // Push the current terminal size on every (re)connect so the
        // negotiator advertises the live cols × rows via NAWS as soon
        // as the server asks. MUDs that honor NAWS wrap at this width
        // server-side, which is the right answer to word wrap.
        if (payload.kind === 'connected') {
          const handle = termRef.current;
          if (handle) {
            const { cols, rows } = handle.getSize();
            void setWindowSize(cols, rows).catch(() => {});
          }
        }
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

  const grouped = groupPanels(panelLayout);
  // Tick and mud time render once, in the LineChip mounted inside the
  // input row. They no longer participate in panel placement.
  const renderPanel = (id: PanelId) => {
    switch (id) {
      case 'map':
        return <MapPane key="map" />;
      case 'group':
        return <GroupPane key="group" />;
      case 'vitals':
        return <VitalsBar key="vitals" />;
      case 'roomstrip': {
        const placement = panelLayout.placements.roomstrip;
        const inSideZone = placement.zone === 'left' || placement.zone === 'right';
        if (!inSideZone) return <RoomStrip key="roomstrip" />;
        // Side-zone layout: wrap content + drop the horizontal
        // scrollbar so a packed room reads top-to-bottom instead of
        // being clipped by the panel width. Wrap in a Resizable so
        // the user can give it more height when they want every
        // chip + name visible without scrolling at all.
        const anchor: 'top' | 'bottom' = placement.align === 'top' ? 'top' : 'bottom';
        return (
          <Resizable
            key="roomstrip"
            storageKey="vosh.layout.roomstrip.height"
            anchor={anchor}
            defaultSize={140}
            minSize={48}
            maxSize={800}
            reservePx={120}
            handleLabel="resize roomstrip panel"
          >
            <RoomStrip variant="column" />
          </Resizable>
        );
      }
      case 'chat':
        return <ChatPane key="chat" onClose={() => setPanelZone('chat', 'hidden')} />;
      case 'affects':
        return <AffectsBar key="affects" />;
    }
  };

  // Side-zone composition. Three slots stacked vertically:
  //   1. Top cluster   — non-fill panels with align=top (natural size,
  //                      hugs the top edge).
  //   2. Middle slot   — either fill panels (map) growing to absorb
  //                      remaining height, or a plain spacer when no
  //                      fill panel is in this zone.
  //   3. Bottom cluster— non-fill panels with align=bottom (natural
  //                      size, hugs the bottom edge).
  //
  // The middle slot is what guarantees the top cluster sticks to the
  // top edge and the bottom cluster sticks to the bottom edge even
  // without a fill panel.
  const renderSideZoneInner = (top: PanelId[], bottom: PanelId[]) => {
    const isFill = (id: PanelId) => Boolean(PANELS[id].fillsSideZone);
    const fillIds = [...top, ...bottom].filter(isFill);
    const fixedTop = top.filter((id) => !isFill(id));
    const fixedBottom = bottom.filter((id) => !isFill(id));
    const hasAny = fillIds.length > 0 || fixedTop.length > 0 || fixedBottom.length > 0;
    if (!hasAny) return null;
    return (
      <div className="panel-zone-stack">
        {fixedTop.length > 0 && (
          <div className="panel-zone-substack panel-zone-substack-fixed">
            {fixedTop.map(renderPanel)}
          </div>
        )}
        {fillIds.length > 0 ? (
          <div className="panel-zone-substack panel-zone-substack-grow">
            {fillIds.map(renderPanel)}
          </div>
        ) : (
          <div className="panel-zone-spacer" />
        )}
        {fixedBottom.length > 0 && (
          <div className="panel-zone-substack panel-zone-substack-fixed">
            {fixedBottom.map(renderPanel)}
          </div>
        )}
      </div>
    );
  };

  const leftHasAny = grouped.leftTop.length > 0 || grouped.leftBottom.length > 0;
  const rightHasAny = grouped.rightTop.length > 0 || grouped.rightBottom.length > 0;

  const inputElement = (
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
        // Esc always snaps the live pane back to the bottom AND
        // closes the split if it is open. So a user who scrolled
        // up via mouse wheel or PageUp gets jumped back to the
        // live tail with one keystroke whether the split is
        // showing or not.
        if (splitOpen) setSplitOpen(false);
        termRef.current?.scrollToBottom();
      }}
    />
  );

  const bottomZoneElement = grouped.bottom.length > 0 && (
    <div className="panel-zone panel-zone-bottom">{grouped.bottom.map(renderPanel)}</div>
  );

  const terminalAreaElement = (
    <div
      ref={terminalAreaRef}
      className={`terminal-area${splitOpen ? ' terminal-area-split' : ''}`}
      onMouseUp={handleTerminalMouseUp}
    >
      {splitOpen && (
        // History pane is a Resizable so the user can drag the
        // divider between history and live to set the split ratio.
        // anchor=top places the panel at the top with the drag
        // handle on the bottom edge facing the live pane below.
        // Lazy mount. A hidden Terminal cannot be measured by
        // FitAddon (its container is display:none, bounding rect
        // 0x0) so writes wrap at 1-3 columns and the scrollback
        // arrives mangled. The initial scroll runs in
        // onScrollbackLoaded — onReady fires before loadScrollback
        // resolves, and a scrollPages call on an empty terminal
        // is a no-op that the next write would override anyway.
        <Resizable
          storageKey="vosh.layout.splitHistoryHeight"
          anchor="top"
          defaultSize={240}
          minSize={80}
          maxSize={1200}
          reservePx={120}
          className="terminal-pane terminal-pane-history"
          handleLabel="resize scrollback split"
        >
          <Terminal
            fontFamily={fontFamily}
            fontSize={fontSize}
            themeTerminalColors={themeTerminalColors}
            quiet
            onReady={(handle) => {
              historyTermRef.current = handle;
            }}
            onScrollbackLoaded={() => {
              historyTermRef.current?.scrollPages(-1);
            }}
            onScrollPosition={(back, max) => setHistoryScrollPos({ back, max })}
          />
          {historyScrollPos && historyScrollPos.max > 0 && (
            <div className="scrollback-indicator" aria-live="polite">
              ↑ {historyScrollPos.back} / {historyScrollPos.max}
            </div>
          )}
        </Resizable>
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
  );

  return (
    <main
      className={`app${sidePanelsFillHeight ? ' app-side-fill' : ''}`}
      onMouseUp={handleAppMouseUp}
    >
      <TopBar
        mapOpen={panelLayout.placements.map.zone !== 'hidden'}
        onToggleMap={() => togglePanelVisibility('map')}
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
        {sidePanelsFillHeight ? (
          <div className="terminal-column">
            {terminalAreaElement}
            {bottomZoneElement}
            {inputElement}
            <StatusBar />
          </div>
        ) : (
          terminalAreaElement
        )}
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
      {!sidePanelsFillHeight && bottomZoneElement}
      {!sidePanelsFillHeight && inputElement}
      {!sidePanelsFillHeight && <StatusBar />}
      <UpdateNotice />
    </main>
  );
}

export default App;
