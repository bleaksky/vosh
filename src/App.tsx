import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { Terminal, nativeSurfaceEnabled, type TerminalHandle } from './components/Terminal';
import { Input, type InputHandle } from './components/Input';
import { type ConnectionStatus } from './components/Connect';
import { TopBar } from './components/TopBar';
import { StatusBar } from './components/StatusBar';
import { Resizable } from './components/Resizable';
import { AffectsBar } from './components/AffectsBar';
import { MapPane } from './components/MapPane';
import { ChatPane } from './components/ChatPane';
import { GroupPane } from './components/GroupPane';
import { ImmPane } from './components/ImmPane';
import { startImmStore } from './lib/immStore';
import { RoomStrip } from './components/RoomStrip';
import { VitalsBar, CombatPane } from './components/VitalsBar';
import { UpdateNotice } from './components/UpdateNotice';
import { HelpView } from './components/HelpView';
import { FindToolbar, type FindToolbarHandle } from './components/FindToolbar';
import {
  broadcastUiConfigChanges,
  dockLayoutGet,
  dockLayoutSet,
  getUiConfig,
  setWindowSize,
  listTriggers,
  resolveThemeTerminalColors,
  onState,
  presetsInstall,
  presetsRemove,
  subscribeBrightBoldChanged,
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
  // The native surface draws its own divider; keep it in the same color.
  if (nativeSurfaceEnabled()) {
    void invoke('native_surface_set_divider_color', { color }).catch(() => {});
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
  // Write frontend-generated terminal content (custom prompt, local echo,
  // error notices) to xterm AND mirror it into the native grid. The native
  // surface renders only the backend display stream, so frontend-only
  // writes are invisible there unless we feed them in too.
  const writeLive = (text: string) => {
    termRef.current?.write(text);
    if (nativeSurfaceEnabled()) {
      void invoke('native_surface_echo', { text }).catch(() => {});
    }
  };
  // Report the bright-bold setting to the native surface (xterm has no
  // equivalent option, so this drives the GPU renderer only).
  const applyBrightBold = (on: boolean) => {
    if (nativeSurfaceEnabled()) {
      void invoke('native_surface_set_bright_bold', { on }).catch(() => {});
    }
  };
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
  // In-client help modal. Opened from the top-bar [help] button;
  // backdrop click, [close], and Esc all dismiss via setHelpOpen(false).
  const [helpOpen, setHelpOpen] = useState(false);
  // Scrollback find toolbar. Opens on Cmd+F (macOS) or Ctrl+F (other
  // platforms). Drives xterm's SearchAddon. The live pane always owns
  // iteration (selection + cached search term live on its addon, so
  // pressing Enter advances one match each time). The history pane
  // mirrors the search in parallel so a scrollback match has a
  // visible highlight up top while the live pane stays anchored to
  // its tail. A search whose match lands inside the live viewport
  // skips the split entirely.
  const [findOpen, setFindOpen] = useState(false);
  const findToolbarRef = useRef<FindToolbarHandle | null>(null);
  // History pane readiness: flips true once the history Terminal has
  // finished loading scrollback after its mount. We queue any pending
  // mirror search through pendingFindRef until the history is ready,
  // since findNext on an empty buffer would silently return no match.
  const [historyReady, setHistoryReady] = useState(false);
  // Live-pane row count captured at the moment the split opens, before
  // the live pane refits to its post-split smaller height. Used by
  // onScrollbackLoaded to position the history viewport so its bottom
  // row is the line that was immediately above the live pane's top
  // row — i.e. opening the split produces zero apparent motion.
  // Reading termRef.getSize().rows inside onScrollbackLoaded was
  // unreliable: that callback may fire before or after the live pane
  // refits, and the answer is different in each case.
  const preSplitLiveRowsRef = useRef(0);
  // TEMPORARY: counts split opens for the blank-split diagnostic log.
  const splitDebugCountRef = useRef(0);
  // On-screen readout of the history pane internals for diagnosing a
  // blank split from a screenshot without devtools. Off unless turned on
  // with localStorage.setItem('vosh.splitdebug', '1').
  const splitDebugEnabled =
    typeof localStorage !== 'undefined' && localStorage.getItem('vosh.splitdebug') === '1';
  const [splitDebug, setSplitDebug] = useState<string | null>(null);
  const pendingFindRef = useRef<{
    query: string;
    opts: { caseSensitive?: boolean; wholeWord?: boolean; regex?: boolean };
    direction: 'next' | 'previous';
  } | null>(null);
  // Match count from live's SearchAddon. Drives the "3 / 12" badge in
  // the find toolbar. `index` of -1 means the active match was lost
  // (e.g. after the toolbar opened but before the first search ran).
  const [findResults, setFindResults] = useState<{ index: number; count: number }>({
    index: -1,
    count: 0,
  });
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

  // Reset history readiness whenever the split closes. The next time
  // the split opens, the history Terminal remounts and the
  // onScrollbackLoaded callback will set this back to true.
  useEffect(() => {
    if (!splitOpen) setHistoryReady(false);
  }, [splitOpen]);

  // Reveal the split as soon as its scrollback lands, not on a fixed
  // timer. The history pane's xterm is held at `visibility: hidden` (the
  // priming class) until `historyReady` flips true, which normally
  // happens in onScrollbackLoaded — but that runs off an xterm
  // write-drain callback that intermittently never fires, stranding the
  // pane hidden and blank. So poll each frame: the moment the buffer
  // holds real content, position it, reveal it, and repaint a few frames
  // (the DOM renderer otherwise leaves the freshly shown rows blank).
  // A ~1.5s ceiling reveals it anyway so an empty or never-arriving
  // buffer can't strand it hidden. Idempotent with the onScrollbackLoaded
  // path, which still runs when it does fire.
  useEffect(() => {
    if (!splitOpen) return;
    let raf = 0;
    let done = false;
    let tries = 0;
    const repaintBurst = () => {
      let frames = 0;
      const repaint = () => {
        historyTermRef.current?.refresh();
        if (++frames < 6) requestAnimationFrame(repaint);
      };
      requestAnimationFrame(repaint);
    };
    const tick = () => {
      const h = historyTermRef.current;
      if (h && !done) {
        const d = h.debug();
        if (d.bufferLength > d.rows + 1) {
          done = true;
          const scrollBack = preSplitLiveRowsRef.current;
          h.scrollToBottom();
          if (scrollBack > 0) h.scrollLines(-scrollBack);
          setHistoryReady(true);
          repaintBurst();
          return;
        }
      }
      if (++tries < 90) {
        raf = requestAnimationFrame(tick);
      } else {
        setHistoryReady(true);
        repaintBurst();
      }
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [splitOpen]);

  // Drain a queued search once the split has opened and the history
  // pane finishes loading scrollback. submitFind enqueues here when a
  // live-pane search would have scrolled the live pane off its tail —
  // we hand the search off to the history pane and run it as soon as
  // history is ready to receive it.
  useEffect(() => {
    if (!splitOpen || !historyReady) return;
    const pending = pendingFindRef.current;
    if (!pending) return;
    pendingFindRef.current = null;
    const handle = historyTermRef.current;
    if (!handle) return;
    if (pending.direction === 'next') handle.findNext(pending.query, pending.opts);
    else handle.findPrevious(pending.query, pending.opts);
  }, [splitOpen, historyReady]);

  // Sync selections between the live and history panes so only one
  // can be active at a time. Without this, dragging a selection in
  // the history pane while a stale live-pane selection lingers
  // produces two simultaneous selections that compete for the copy
  // shortcut (the live pane's wins) — confusing the user who only
  // sees the history-pane highlight. Reactive cross-clear means
  // the most recent gesture is always the one that "owns" the
  // selection.
  useEffect(() => {
    if (!historyReady) return;
    const live = termRef.current;
    const hist = historyTermRef.current;
    if (!live || !hist) return;
    const unsubLive = live.onSelectionChange(() => {
      if (live.hasSelection()) hist.clearSelection();
    });
    const unsubHist = hist.onSelectionChange(() => {
      if (hist.hasSelection()) live.clearSelection();
    });
    return () => {
      unsubLive();
      unsubHist();
    };
  }, [historyReady]);

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
    // Accumulate raw deltaY so high-frequency touchpad events
    // (~60 small deltas/sec on macOS) don't compound into a runaway
    // scroll. Each PX_PER_LINE pixels of accumulated delta = one
    // line scrolled in the history pane; CRITICAL: we
    // preventDefault on every event we're "handling" — even when
    // the accumulator hasn't ticked over a line yet — otherwise
    // small touchpad deltas leak through to xterm and scroll the
    // LIVE pane while the user thinks they're scrolling history.
    const PX_PER_LINE = 12;
    let wheelAccum = 0;
    const onWheel = (e: globalThis.WheelEvent) => {
      if (e.deltaY === 0) return;
      const scrollingUp = e.deltaY < 0;
      const splitOpen = splitOpenRef.current;
      // Decide whether this event belongs to us or to xterm's live
      // pane handler: up-scroll always belongs to us (it opens the
      // split or scrolls history); down-scroll belongs to us only
      // when the split is already open. Anything else falls
      // through to xterm.
      const ours = scrollingUp || splitOpen;
      if (!ours) return;
      e.preventDefault();
      e.stopPropagation();
      // Direction change resets the accumulator so a fresh swipe
      // doesn't inherit leftover delta from the previous direction.
      if (wheelAccum !== 0 && Math.sign(e.deltaY) !== Math.sign(wheelAccum)) {
        wheelAccum = 0;
      }
      // First up-scroll opens the split without consuming the
      // accumulator — gives the user a single "intent" gesture
      // before history starts moving.
      if (scrollingUp && !splitOpen) {
        preSplitLiveRowsRef.current = termRef.current?.getSize().rows ?? 0;
        setSplitOpen(true);
        wheelAccum = 0;
        return;
      }
      wheelAccum += e.deltaY;
      const lines = Math.trunc(wheelAccum / PX_PER_LINE);
      if (lines === 0) return;
      wheelAccum -= lines * PX_PER_LINE;
      historyTermRef.current?.scrollLines(lines);
      if (lines > 0) {
        queueMicrotask(() => {
          if (historyTermRef.current?.isAtBottom()) setSplitOpen(false);
        });
      }
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

  // Cmd+F (macOS) / Ctrl+F (others) opens the scrollback find toolbar.
  // Attached at the capture phase so it fires before xterm's own
  // keybindings or the webview's default find dialog. A second press
  // while the toolbar is already open re-focuses + selects the input
  // so the user can type a new query immediately.
  //
  // Skipped only when the help modal is open (its own search box is
  // already taking that role). In every other context — including
  // while focus sits in the command input, which is the common case —
  // Cmd+F opens the scrollback search.
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key.toLowerCase() !== 'f') return;
      const primary = e.metaKey || e.ctrlKey;
      if (!primary || e.altKey) return;
      if (helpOpen) return;
      e.preventDefault();
      e.stopPropagation();
      if (findOpen) {
        findToolbarRef.current?.focus();
      } else {
        // Open just the toolbar. Whether to open the split is decided
        // per-search: only when a match would scroll the live pane up
        // off its tail (see submitFind below).
        setFindOpen(true);
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [findOpen, helpOpen]);

  // Esc closes the find toolbar regardless of which element has
  // focus. The toolbar's own Esc handler is attached to its input,
  // so a click that drifted focus away (or the split auto-closing
  // when history scrolled back to its tail) would otherwise leave
  // the user pressing Esc to no effect. Capture-phase + stopPropagation
  // keeps this from also firing Input.tsx's split-close handler.
  useEffect(() => {
    if (!findOpen) return;
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.preventDefault();
      e.stopPropagation();
      termRef.current?.clearSearch();
      historyTermRef.current?.clearSearch();
      if (nativeSurfaceEnabled()) {
        void invoke('native_surface_find_clear').catch(() => {});
      }
      pendingFindRef.current = null;
      setFindResults({ index: -1, count: 0 });
      setFindOpen(false);
      inputRef.current?.focus();
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [findOpen]);

  // The native surface is opaque and on top, so DOM popovers (dropdowns,
  // menus, modals) that overlap the terminal would be occluded by it. Watch
  // for them and hide the surface while any are open; xterm renders the same
  // content behind it, so the swap is seamless. Overlays that do not carry a
  // standard role can opt in with data-occludes-surface.
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    const selector = '[role="menu"],[role="listbox"],[role="dialog"],[data-occludes-surface]';
    let occluded = false;
    const check = () => {
      const present = document.querySelector(selector) !== null;
      if (present !== occluded) {
        occluded = present;
        void invoke('native_surface_set_visible', { visible: !present }).catch(() => {});
      }
    };
    const observer = new MutationObserver(check);
    observer.observe(document.body, { childList: true, subtree: true });
    check();
    return () => {
      observer.disconnect();
      void invoke('native_surface_set_visible', { visible: true }).catch(() => {});
    };
  }, []);

  // Bootstrap the chat + group + imm buffers at app launch so any
  // Comm.Channel / routed / Group.Info / Char.Worth / Imm.Queues
  // pushes that arrive while the panes are closed (or have not yet
  // been opened) still land in the stores and are visible on first
  // open. Imm.Queues especially: the server sends the only guaranteed
  // snapshot at login and has no heartbeat, so a lazily-started store
  // would drop it and the panel would sit dark until the next queue
  // change.
  useEffect(() => {
    startChatStore();
    startGroupStore();
    startImmStore();
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
        setThemeTerminalColors(resolveThemeTerminalColors(cfg.theme, cfg.theme_terminal_colors));
        applyBrightBold(cfg.bright_bold);
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
          setThemeTerminalColors(resolveThemeTerminalColors(cfg.theme, cfg.theme_terminal_colors));
          applyBrightBold(cfg.bright_bold);
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
    // Settings save broadcasts the bright-bold toggle. Apply it to the
    // native surface without a relaunch.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    subscribeBrightBoldChanged((value) => {
      applyBrightBold(value);
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

  // Clicks on the native surface are eaten by the opaque view, so the
  // backend emits an event on mouse-up and the input focuses here —
  // matching the DOM mouseup handler that covers the rest of the window.
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen('vosh://terminal-clicked', () => {
      inputRef.current?.focus();
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // The custom prompt renders in the BACKEND (prompt_template.rs) so the
  // gag-erase and the replacement land in one output batch — rendering it
  // here off the prompt-vars event put an IPC round trip between the two
  // and every prompt flashed a blank row. The rendered prompt reaches both
  // renderers through the normal session output stream.

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
          writeLive(`\r\n\x1b[31m[${payload.reason}]\x1b[0m\r\n`);
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

  // Run a find call from the toolbar. The live pane is the
  // authoritative iterator: each call advances its SearchAddon
  // selection + cachedSearchTerm, so pressing Enter walks through
  // matches in order. The history pane is a passive mirror, used
  // only when the active match falls outside the live viewport.
  //
  // Strategy per call:
  //   1. Advance live.findNext (or findPrevious). If no match, close
  //      any open split and clear history decorations.
  //   2. If after the call live is still anchored to its tail, the
  //      match is in the visible viewport. Close the split if it had
  //      been opened for a prior scrollback match.
  //   3. Otherwise the active match is up in scrollback. Snap live
  //      back to its tail (without clearing live's search state, so
  //      iteration survives), open the split, and run the same search
  //      on the history pane so its decorations + viewport land on
  //      a matching line.
  const submitFind = (
    query: string,
    opts: { caseSensitive?: boolean; wholeWord?: boolean; regex?: boolean },
    direction: 'next' | 'previous',
  ): boolean => {
    if (query.length === 0) return false;

    // Native surface: the grid owns search, scroll-to-match, and the
    // highlight. Route to the native command and feed the count back to
    // the toolbar; no xterm split is involved.
    if (nativeSurfaceEnabled()) {
      void invoke<[number, number]>('native_surface_find', {
        query,
        regex: opts.regex ?? false,
        caseSensitive: opts.caseSensitive ?? false,
        wholeWord: opts.wholeWord ?? false,
        forward: direction === 'next',
      })
        .then(([current, total]) => {
          setFindResults({ index: total > 0 ? current - 1 : -1, count: total });
        })
        .catch(() => {});
      return true;
    }

    const live = termRef.current;
    if (!live) return false;

    const hit = direction === 'next' ? live.findNext(query, opts) : live.findPrevious(query, opts);
    if (!hit) {
      if (splitOpen) {
        historyTermRef.current?.clearSearch();
        setSplitOpen(false);
      }
      return false;
    }

    if (live.isAtBottom()) {
      // Match landed inside the live viewport. Decorations on live
      // are visible; no split needed. Tear down the split if it had
      // been opened for an earlier scrollback match.
      if (splitOpen) {
        historyTermRef.current?.clearSearch();
        setSplitOpen(false);
      }
      return true;
    }

    // Match is up in scrollback. Pin live back to its tail so it
    // keeps streaming; live's SearchAddon selection + cachedSearchTerm
    // survive the scroll, which is what lets the next call advance.
    // Mirror the search into the history pane so the user can see
    // the highlighted match up there.
    live.scrollToBottom();
    if (!splitOpen) {
      pendingFindRef.current = { query, opts, direction };
      setSplitOpen(true);
    } else if (historyTermRef.current && historyReady) {
      if (direction === 'next') historyTermRef.current.findNext(query, opts);
      else historyTermRef.current.findPrevious(query, opts);
    } else {
      pendingFindRef.current = { query, opts, direction };
    }
    return true;
  };

  const handleError = (message: string) => {
    setStatus({ kind: 'error', message });
    writeLive(`\r\n\x1b[31m[${message}]\x1b[0m\r\n`);
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
      case 'imm':
        return <ImmPane key="imm" />;
      case 'vitals': {
        // Combat panel set to `hidden` = inline within the vitals bar
        // (the legacy default). Any other zone routes combat through
        // its standalone CombatPane in that zone, so suppress the
        // inline chip to avoid double-rendering.
        const combatZone = panelLayout.placements.combat.zone;
        const hideCombat = combatZone !== 'hidden';
        return <VitalsBar key="vitals" hideCombat={hideCombat} />;
      }
      case 'combat': {
        // Chip variant only when combat sits IMMEDIATELY ABOVE vitals
        // — both must be in the bottom zone for them to read as one
        // attached block. Anywhere else (including bottom-alone) the
        // pane uses the full treatment with content vertically
        // centered in its own surface.
        const combatZone = panelLayout.placements.combat.zone;
        const vitalsZone = panelLayout.placements.vitals.zone;
        const chip = combatZone === 'bottom' && vitalsZone === 'bottom';
        return <CombatPane key="combat" chip={chip} />;
      }
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
      case 'chat': {
        // Wrap chat in Resizable so the user can grow / shrink it
        // from whichever edge faces the sibling content. Anchor maps
        // to the zone: bottom zone -> bottom anchor (handle on top),
        // top zone -> top anchor (handle on bottom), side zones use
        // align (top/bottom) the same way RoomStrip does.
        const chatPlacement = panelLayout.placements.chat;
        const chatAnchor: 'top' | 'bottom' =
          chatPlacement.zone === 'top'
            ? 'top'
            : chatPlacement.zone === 'bottom'
              ? 'bottom'
              : chatPlacement.align === 'top'
                ? 'top'
                : 'bottom';
        return (
          <Resizable
            key="chat"
            storageKey="vosh.layout.chat.height"
            anchor={chatAnchor}
            defaultSize={160}
            minSize={80}
            maxSize={800}
            reservePx={160}
            handleLabel="resize chat panel"
          >
            <ChatPane onClose={() => setPanelZone('chat', 'hidden')} />
          </Resizable>
        );
      }
      case 'affects': {
        // Side-zone placement gets the Ember pane treatment (pane
        // head + per-affect duration rows); top/bottom keep the
        // horizontal pill strip.
        const zone = panelLayout.placements.affects.zone;
        const rows = zone === 'left' || zone === 'right';
        return <AffectsBar key="affects" variant={rows ? 'rows' : 'strip'} />;
      }
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
      onLocalEcho={(text) => {
        writeLive(text);
        // Mirror to the split history pane so typed lines appear
        // there too. Without this, scrolling up in split view
        // shows server output but none of your own commands. The
        // history Terminal is lazy-mounted and may be null between
        // splitOpen=true and onReady; the optional chain absorbs
        // that gap.
        historyTermRef.current?.write(text);
      }}
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
      className={`terminal-area${splitOpen ? ' terminal-area-split' : ''}${
        findOpen && nativeSurfaceEnabled() ? ' terminal-area-find-inset' : ''
      }`}
      onMouseUp={handleTerminalMouseUp}
    >
      {findOpen && (
        <FindToolbar
          ref={findToolbarRef}
          results={findResults}
          onFindNext={(query, opts) => submitFind(query, opts, 'next')}
          onFindPrevious={(query, opts) => submitFind(query, opts, 'previous')}
          onClose={() => {
            termRef.current?.clearSearch();
            historyTermRef.current?.clearSearch();
            if (nativeSurfaceEnabled()) {
              void invoke('native_surface_find_clear').catch(() => {});
            }
            pendingFindRef.current = null;
            setFindResults({ index: -1, count: 0 });
            setFindOpen(false);
            inputRef.current?.focus();
          }}
        />
      )}
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
          className={`terminal-pane terminal-pane-history${historyReady ? '' : ' terminal-pane-history-priming'}`}
          handleLabel="resize scrollback split"
          snapPx={() => termRef.current?.cellHeight() ?? 0}
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
              // Position the history pane so its bottom row is the
              // line immediately above the live pane's full row
              // range. The live pane is `position: absolute` with
              // both top and bottom pinned (overlay model), so its
              // xterm renders the full terminal-area row count even
              // while the history overlay covers part of it. If
              // history's bottom landed inside live's row range the
              // same lines would render in both panes — opaque
              // overlay hides that visually, but the scroll-depth
              // indicator still makes more sense when the panes
              // describe disjoint buffer regions. Pre-split live
              // rows captured in the wheel handler because reading
              // the live pane's size here is racey.
              // Fast path: when the load callback fires, position and
              // reveal immediately. The frame-polled effect above also
              // positions / reveals / repaints, so this is idempotent and
              // a no-op when the callback never fires.
              const scrollBack = preSplitLiveRowsRef.current;
              if (scrollBack > 0) historyTermRef.current?.scrollLines(-scrollBack);
              setHistoryReady(true);
              // Optional split diagnostic, enabled via
              // localStorage.setItem('vosh.splitdebug', '1'). Snapshots
              // the history pane internals into the on-screen overlay.
              if (splitDebugEnabled) {
                const openN = (splitDebugCountRef.current += 1);
                const lines: string[] = [`open#${openN} scrollBack=${scrollBack}`];
                const snap = (tag: string) => {
                  const d = historyTermRef.current?.debug();
                  const line = `${tag} ${JSON.stringify(d)}`;
                  // eslint-disable-next-line no-console
                  console.log(`[vosh-split-debug] open#${openN} ${line}`);
                  lines.push(line);
                  setSplitDebug(lines.join('\n'));
                };
                snap('t0');
                window.setTimeout(() => snap('t250'), 250);
                window.setTimeout(() => snap('t600'), 600);
              }
            }}
            onScrollPosition={(back, max) => setHistoryScrollPos({ back, max })}
          />
          {historyScrollPos && historyScrollPos.max > 0 && (
            <div className="scrollback-indicator" aria-live="polite">
              ↑ {historyScrollPos.back} / {historyScrollPos.max}
            </div>
          )}
          {splitDebugEnabled && splitDebug && (
            <pre className="split-debug-overlay">{splitDebug}</pre>
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
          onResultsChanged={(event) =>
            setFindResults({ index: event.resultIndex, count: event.resultCount })
          }
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
        onOpenHelp={() => setHelpOpen(true)}
        connectionStatus={status}
        onConnectionError={handleError}
      />
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
      {helpOpen && <HelpView onClose={() => setHelpOpen(false)} />}
    </main>
  );
}

export default App;
