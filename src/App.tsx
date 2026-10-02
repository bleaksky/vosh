import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import {
  NATIVE_FAILED_KEY,
  Terminal,
  nativeSurfaceEnabled,
  nativeUnderlay,
  type TerminalHandle,
} from './components/Terminal';
import { Input, type InputHandle } from './components/Input';
import { Resizable } from './components/Resizable';
import { UpdateNotice } from './components/UpdateNotice';
import { Toasts } from './components/Toasts';
import { FindToolbar, type FindToolbarHandle } from './components/FindToolbar';
import { TerminalMenu } from './components/TerminalMenu';
import { ScrollDepth } from './components/ScrollDepth';
import { AppShell } from './components/shell/AppShell';
import { TitleBand } from './components/shell/TitleBand';
import { StatusLine } from './components/shell/StatusLine';
import { PanelHost } from './components/panel/PanelHost';
import {
  panelWidthOf,
  setPanelWidth,
  togglePanelOpen,
  updatePanelLayout,
  usePanelLayout,
} from './components/panel/panelLayoutStore';
import {
  disconnectSession,
  followReplacedUiConfig,
  getUiConfig,
  setWindowSize,
  listTriggers,
  resolveThemeTerminalColors,
  onState,
  presetsInstall,
  presetsRemove,
  subscribeBrightBoldChanged,
  subscribeBaseAnsiChanged,
  subscribeCustomThemesChanged,
  subscribeMigrationApplied,
  subscribeSplitDividerChanged,
  subscribeTerminalLineHeightChanged,
  normalizeTerminalLineHeight,
  terminalLocalWrite,
  promptConfigGet,
  promptConfigSet,
  promptCodeReaderSet,
  promptPreviewSet,
  subscribePromptCardOpen,
  TERMINAL_LINE_HEIGHTS,
  type StatePayload,
  type TerminalLineHeight,
} from './lib/session';
import {
  applyAndBroadcastTheme,
  applyThemePrefs,
  getCurrentThemeId,
  subscribeThemePrefs,
} from './lib/theme';
import { loadFontStack, renderFontStack } from './lib/fontLoader';
import { PRESETS, presetTriggers } from './lib/presets';
import { presetLaunchPlan } from './lib/automationRecords';
import { listenForQuitFlush } from './lib/pendingWrites';
import { customToAppTheme, findTheme, setCustomThemes, themeTokens } from './lib/themes';
import { parseHex, toRgba } from './lib/color';
import { setBaseAnsi } from './lib/baseAnsi';
import { startStores } from './lib/stores';
import { pushToast } from './lib/toasts';
import { showLaunchNotices, showMigrationApplied } from './lib/launchNotices';
import { startGamePromptToasts } from './lib/gamePromptToast';
import { CommandPalette } from './components/CommandPalette';
import {
  buildPaletteEntries,
  isMacPlatform,
  shortcutKey,
  themeEntries,
  themesInGalleryOrder,
  type PaletteDeps,
} from './lib/palette';
import {
  buildMenuState,
  commandRepeats,
  listenAppMenu,
  menuCopy,
  pageHasSelection,
  requestSessionMenu,
  resolveShortcut,
  setAppMenuState,
} from './lib/appMenu';
import { getImmState, subscribeImmState } from './lib/immStore';
import { ConfirmDialog } from './components/ConfirmDialog';
import { openSettingsTab, openSettingsWindow } from './lib/settingsLink';
import {
  HELP_OPEN_EVENT,
  helpNoMatchNotice,
  helpOpensOn,
  openHelpTopic,
  openHelpWindow,
} from './lib/helpLink';
import { showAfterThemePaint } from './lib/reveal';
import { getNativeScroll, startNativeScroll, subscribeNativeScroll } from './lib/nativeScroll';
import {
  addPane,
  allPanes,
  closePane,
  isLeaf,
  PANE_TYPES,
  type PaneNode,
  type PaneType,
} from './lib/paneLayout';
import { offeredPaneTypes } from './components/panel/paneTypes';
import { useConnection, type ConnectionStatus } from './lib/useConnection';
import { useEscape } from './lib/escapeStack';
import { usePromptShow } from './lib/promptShow';
import { PromptDock } from './components/prompt/PromptDock';
import { PromptCard, type PromptCardHost } from './components/prompt/PromptCard';
import { nextCardRequest, type CardRequest, type CardRequestView } from './lib/promptCard';
import { notePageWrite, usePinnedDockRows } from './lib/stores/pinnedPromptStore';
import { usePromptReach } from './lib/stores/promptReachStore';
import { lentRows, type CellSize } from './lib/promptBand';
import { noteReader } from './lib/readerBusy';

const RENAME_MIGRATION_KEY = 'vosh.migration.from_mudclient';

// Hide or show the panel. When focus sat on the title band's toggle or
// inside the panel, the caret goes back to the command line: a hidden
// panel is inert and would drop focus to the body, and a toggle button
// that keeps focus (WebView2 and WebKitGTK focus on click) would take
// the next Space and toggle again.
function togglePanelKeepingCaret(focusInput: () => void): void {
  const active = document.activeElement;
  const stranded =
    active instanceof Element && active.closest('.shell-slot-panel, .shell-band') !== null;
  togglePanelOpen();
  if (stranded) focusInput();
}

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

// Hand the native surface the chrome colors the page derives with its
// theme tokens: the split divider, the selection, find matches in ANSI
// yellow (28% for every match as Menus.dc.html draws them, stronger for
// the current one), links in the accent, and the scrollbar in the
// tertiary tone. A lifted prompt's band takes the selected row fill, with
// its inset ring on a light theme. Runs on every theme apply, so light
// themes never get the renderer's dark defaults.
function pushNativeChromeTokens(): void {
  const theme = findTheme(getCurrentThemeId());
  const tokens = themeTokens(theme);
  const yellow = parseHex(theme.xterm.yellow);
  void invoke('native_surface_set_tokens', {
    divider: tokens.sep,
    selection: tokens.selection,
    findMatch: yellow ? toRgba(yellow, 0.28) : null,
    currentMatch: yellow ? toRgba(yellow, 0.6) : null,
    link: tokens.accent,
    scrollbar: tokens.tertiary,
    selrow: tokens.selrow,
    appearance: tokens.appearance,
  }).catch(() => {});
}

// The id of the leaf showing `pane`, or null when the panel does not
// show it.
function leafIdFor(node: PaneNode, pane: PaneType): string | null {
  if (isLeaf(node)) return node.pane === pane ? node.id : null;
  for (const child of node.children) {
    const hit = leafIdFor(child, pane);
    if (hit !== null) return hit;
  }
  return null;
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

const DEFAULT_FONT_FAMILY = '"JetBrainsMono Bundled", Menlo, Consolas, ui-monospace, monospace';

function App() {
  const [status, setStatus] = useState<ConnectionStatus>({ kind: 'idle' });
  // Boot with the last-known font instead of the compiled default.
  // The real value arrives async from the Rust config; booting on the
  // default and flipping when config lands rescales the whole input
  // row a beat after every page load, and the next keystroke visibly
  // shifts the layout as stale heights correct themselves.
  const [fontFamily, setFontFamily] = useState(() => {
    try {
      return localStorage.getItem('vosh.cache.fontFamily') || DEFAULT_FONT_FAMILY;
    } catch {
      return DEFAULT_FONT_FAMILY;
    }
  });
  // The list the terminal draws with, the one the native atlas walks.
  const renderFamily = useMemo(() => renderFontStack(fontFamily), [fontFamily]);
  const [fontSize, setFontSize] = useState(() => {
    try {
      const n = Number(localStorage.getItem('vosh.cache.fontSize'));
      return Number.isFinite(n) && n >= 6 && n <= 64 ? n : 14;
    } catch {
      return 14;
    }
  });
  // Cached like the font so the first paint uses the saved row spacing
  // instead of reflowing once the config arrives.
  const [terminalLineHeight, setTerminalLineHeight] = useState<TerminalLineHeight>(() => {
    try {
      return normalizeTerminalLineHeight(localStorage.getItem('vosh.cache.lineHeight'));
    } catch {
      return 'default';
    }
  });
  const [themeTerminalColors, setThemeTerminalColors] = useState(false);
  // The panel's open state, width, and pane tree, per profile. The
  // panel's layout store is the one copy in this window. The title
  // band and the palette show, hide, and size the panel through it,
  // and the panel edits the tree through it.
  const panelLayout = usePanelLayout();
  // Where your prompt shows, and whether this profile reads one.
  const promptShow = usePromptShow();
  const promptPinned = promptShow?.show === 'pinned' && promptShow.capture;
  const promptLifted = promptShow?.show === 'lifted' && promptShow.capture;
  // The cell the live terminal draws at, which the pinned band lays its
  // characters out on.
  const [cellSize, setCellSize] = useState<CellSize | null>(null);
  // The rows the pinned band shows now, and the rows past its first it
  // borrows from the bottom of the live terminal while the dock shows.
  // This reads the same store as the dock, so both change in one commit.
  // While the dock shows, the live terminal also keeps its grid to the
  // bottom of its pane, so its newest line sits the dock's gap over the
  // band in any window.
  const dockShown = usePinnedDockRows(promptShow?.zone ?? 1, promptShow?.promptsOff ?? false);
  const dockShows = promptPinned && cellSize !== null;
  const dockLent = dockShows ? lentRows(dockShown) : 0;
  // Bright bold, which the native grid and the pinned band over it follow.
  const [brightBold, setBrightBold] = useState(false);
  const panelOpen = panelLayout?.panel_open ?? true;
  const panelWidth = panelWidthOf(panelLayout);
  const shownPanes = useMemo(() => (panelLayout ? allPanes(panelLayout.root) : []), [panelLayout]);
  const termRef = useRef<TerminalHandle | null>(null);
  const historyTermRef = useRef<TerminalHandle | null>(null);
  const inputRef = useRef<InputHandle | null>(null);
  // Write text the page draws itself (your typed echo, error notices) to
  // xterm, and through terminal_local_write to the native grid and the
  // session on either renderer. The session closes the open row, since
  // the text now follows it, so it never repaints over your echo. Your
  // line goes out by its own call, so the session can hear of the echo
  // after the reply. It closes only the rows that came before the newest
  // output the renderer that shows took: xterm names it here, and the
  // native grid names its own as it takes the text.
  const writeLive = (text: string) => {
    notePageWrite(text);
    const term = termRef.current;
    term?.write(text);
    const after = nativeSurfaceEnabled() ? null : (term?.outputTaken() ?? 0);
    void terminalLocalWrite(text, after).catch(() => {});
  };
  const handleError = (message: string) => {
    setStatus({ kind: 'error', message });
    writeLive(`\r\n\x1b[31m[${message}]\x1b[0m\r\n`);
  };
  // The session for the title band, the session menu, the palette, and
  // Cmd+R.
  const connection = useConnection(status, handleError);
  // Report the bright-bold setting to the native surface (xterm has no
  // equivalent option, so this drives the GPU renderer only).
  const applyBrightBold = (on: boolean) => {
    setBrightBold(on);
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
  // ⌘K command palette.
  const [paletteOpen, setPaletteOpen] = useState(false);
  // Close window (⌘W on macOS) while a session is live asks first,
  // since closing the main window ends the session and quits Vosh.
  const [confirmClose, setConfirmClose] = useState(false);
  // What the macOS menu bar mirrors beyond the panel and the session: a
  // tick for every theme apply or custom theme change, whether the MUD
  // offers staff queues, and whether the native grid is scrolled back.
  const [themeTick, setThemeTick] = useState(0);
  const [staffOffered, setStaffOffered] = useState(false);
  const [nativeScrolled, setNativeScrolled] = useState(false);
  // Right-click context menu over the terminal area. Non-null while
  // open; the value is the pointer's viewport position (the menu
  // clamps itself to the window edges).
  const [terminalMenu, setTerminalMenu] = useState<{ x: number; y: number } | null>(null);
  // The prompt card (Customize prompt…), open over your prompt, and the
  // view it opens on, or `point` to open on pointing at your game's line.
  const [promptCard, setPromptCard] = useState<CardRequest | null>(null);
  // Every request counts, so the open card hears a repeat of one.
  const openPromptCard = useCallback(
    (view: CardRequestView) => setPromptCard((prev) => nextCardRequest(prev, view)),
    [],
  );
  // The card draws your design over the band of Lifted in the text.
  const [cardBand, setCardBand] = useState(false);
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

  // A preview the prompt card left on before this window loaded again
  // would go on drawing on your prompt, so the window clears it as it
  // mounts.
  useEffect(() => {
    promptPreviewSet(null).catch((e: unknown) =>
      console.error('[main] clearing the prompt preview failed', e),
    );
    // And the code reader the card chose on another host.
    promptCodeReaderSet(false).catch((e: unknown) =>
      console.error('[main] clearing the code reader failed', e),
    );
  }, []);

  // The prompt card reaches the terminal it sits over through these.
  const promptCardHost = useMemo<PromptCardHost>(
    () => ({
      terminal: () => termRef.current,
      area: () => terminalAreaRef.current,
      dock: () => document.querySelector<HTMLElement>('.prompt-dock'),
    }),
    [],
  );
  const closePromptCard = () => {
    setPromptCard(null);
    setCardBand(false);
    inputRef.current?.focus();
  };

  // Customize… in Settings, and anything else in another window, opens
  // the card here and brings this window forward.
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void subscribePromptCardOpen((request) => {
      openPromptCard(request.view ?? 'design');
      void getCurrentWindow()
        .setFocus()
        .catch(() => {});
    })
      .then((fn) => {
        if (alive) unlisten = fn;
        else fn();
      })
      .catch(() => {});
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [openPromptCard]);

  // On quit the backend asks each window for the writes it holds back,
  // like a pane layout waiting out a splitter drag, before it writes
  // the profile and exits.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    listenForQuitFlush()
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch((e: unknown) => console.error('[main] quit listener failed', e));
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Reset the history-pane scroll-depth indicator whenever the split
  // closes. The history Terminal unmounts and the next mount will fire
  // its own onScrollPosition; keeping the prior value here would flash
  // stale numbers for one paint before being overwritten.
  useEffect(() => {
    if (!splitOpen) setHistoryScrollPos(null);
    splitOpenRef.current = splitOpen;
    // Reading back in the split leaves your prompt's clock as it is.
    noteReader('split', splitOpen);
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

  // Showing, hiding, or resizing the panel changes the terminal
  // column's width. FitAddon's own internal observers do not always
  // pick up the change before xterm draws the next frame, which leaves
  // a stripe of unused space at the edge of the terminal until
  // something else (e.g. a scroll) kicks off a refit. Force a fit
  // after layout has settled, so xterm matches the column at once.
  useEffect(() => {
    const id = requestAnimationFrame(() => {
      termRef.current?.fit();
      historyTermRef.current?.fit();
    });
    return () => cancelAnimationFrame(id);
  }, [panelOpen, panelWidth]);

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
      // Native surface: wheel over the terminal goes to the native
      // view, which scrolls the grid itself. The only wheel events
      // that reach this handler are over the padding gutter around
      // the surface — opening the (occluded, invisible) DOM split
      // from those would flip the split layout class for nothing.
      if (nativeSurfaceEnabled()) return;
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
      // Snapping the scrollback back to the bottom is a "get me back to
      // typing" gesture, so return the caret to the command line rather
      // than leaving focus on the terminal surface.
      inputRef.current?.focus();
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
    // Floating surfaces and the panel edge keep the focus they hold, so
    // a press on a form's label or padding does not yank the caret out.
    if (
      target.closest(
        'button, input, textarea, select, a, label, [role="button"], [role="menu"], [role="dialog"], [role="separator"]',
      )
    )
      return;
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

  // After a copy the caret should land back on the command line. The
  // terminal copy path dispatches `vosh:focus-input` explicitly; the
  // DOM `copy` listener is the catch-all for a browser-native copy of
  // selected terminal text. Both skip when the copy came from a field
  // (the command input, the find box) so that field keeps its focus,
  // and the copy listener defers a frame so the clipboard reads the
  // selection before focus moves off it.
  useEffect(() => {
    const focusInput = () => inputRef.current?.focus();
    const onCopy = () => {
      if ((document.activeElement as HTMLElement | null)?.closest('.input-row, input, textarea'))
        return;
      window.setTimeout(focusInput, 0);
    };
    window.addEventListener('vosh:focus-input', focusInput);
    document.addEventListener('copy', onCopy);
    return () => {
      window.removeEventListener('vosh:focus-input', focusInput);
      document.removeEventListener('copy', onCopy);
    };
  }, []);

  const closeFind = () => {
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
  const closePalette = () => {
    setPaletteOpen(false);
    inputRef.current?.focus();
  };

  // Esc closes the open surface on top and nothing under it (see
  // lib/escapeStack). The find bar closes from anywhere, so a click
  // that drifted focus away (or the split auto-closing when history
  // scrolled back to its tail) still leaves Esc working. The palette
  // handles Esc inside itself, where it first steps back out of a
  // submenu.
  useEscape(findOpen, closeFind);
  useEscape(paletteOpen, closePalette, () => document.querySelector('.ov-palette'));
  useEscape(terminalMenu !== null, () => {
    setTerminalMenu(null);
    inputRef.current?.focus();
  });

  // Window shortcuts, in the capture phase so they fire before xterm's
  // own keybindings, the webview's find and reload, and the command
  // line's macros. macOS binds Cmd only, because Ctrl belongs to your
  // macros there. Windows and Linux bind Ctrl. The keys live in
  // lib/appShortcuts.json, which the macOS menu bar reads too.
  //   Mod+K        command palette (toggles)
  //   Mod+F        find in scrollback (again refocuses the find field)
  //   Mod+R        connect to the saved world. Ctrl+R never reloads the
  //                page on Windows, even while connected.
  //   Mod+,        settings
  //   Mod+/        help
  //   Mod+Shift+L  show or hide the panel
  //   Mod+\        open or close the scrollback split
  // A key this handler takes never reaches the menu bar, and the menu
  // bar sends its commands through runCommand below too, so each press
  // runs once. Keys match through shortcutKey, so a Cyrillic or Greek
  // layout still reaches them by the physical key.
  const shortcutState = useRef({ findOpen, paletteOpen, live: connection.live });
  const runCommandRef = useRef<(id: string, opts?: { repeat?: boolean }) => void>(() => {});

  // Open or close the scrollback split, the keyboard twin of a middle
  // click. The native grid splits itself when it scrolls back, so it
  // pages up into history or snaps back to the tail. xterm mounts the
  // history pane above the live one.
  const toggleSplit = () => {
    if (nativeSurfaceEnabled()) {
      void invoke('native_surface_scroll', { kind: 'toggle' }).catch(() => {});
      return;
    }
    if (splitOpenRef.current) {
      setSplitOpen(false);
      termRef.current?.scrollToBottom();
      return;
    }
    // Same pre-split row capture as the wheel and PageUp paths.
    preSplitLiveRowsRef.current = termRef.current?.getSize().rows ?? 0;
    setSplitOpen(true);
  };
  useEffect(() => {
    const mac = isMacPlatform();
    const onKey = (e: globalThis.KeyboardEvent) => {
      const primary = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
      if (!primary || e.altKey) return;
      const hit = resolveShortcut(shortcutKey(e), e.shiftKey);
      if (!hit) return;
      e.preventDefault();
      e.stopPropagation();
      if (hit.id) runCommandRef.current(hit.id, { repeat: e.repeat });
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, []);

  // `#help <words>` from the command line (src-tauri input::help_query).
  // A topic id or number opens that topic. Other words open Help on its
  // search when a topic matches, and the terminal says so when none does.
  const writeLiveRef = useRef(writeLive);
  writeLiveRef.current = writeLive;
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    listen<string>(HELP_OPEN_EVENT, (event) => {
      const words = typeof event.payload === 'string' ? event.payload.trim() : '';
      if (!helpOpensOn(words)) {
        if (words.length > 0) {
          writeLiveRef.current(`\x1b[38;5;244m${helpNoMatchNotice(words)}\x1b[0m\r\n`);
        }
        return;
      }
      openHelpTopic(words);
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Menu bar commands (macOS only). Each arrives with its palette id.
  useEffect(() => {
    if (!isMacPlatform()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    listenAppMenu((id) => runCommandRef.current(id))
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // The native surface is opaque and on top, so DOM popovers (dropdowns,
  // menus, modals) that overlap the terminal would be occluded by it. Watch
  // for them and hide the surface while any are open; xterm renders the same
  // content behind it, so the swap is seamless. Overlays that do not carry a
  // standard role can opt in with data-occludes-surface.
  useEffect(() => {
    // Under the underlay (macOS) the page draws over the surface, so no
    // overlay needs to hide it. Mark the root so CSS leaves the terminal
    // pane unpainted and hides the xterm copy.
    if (nativeUnderlay()) {
      // Leave the pane transparent only once the backend confirms the
      // surface is up. It installs during setup, usually before this runs,
      // so poll briefly. If it never comes up, reload onto xterm for the
      // rest of the session instead of showing a see-through hole.
      let cancelled = false;
      let tries = 0;
      const check = () => {
        void invoke<boolean>('native_surface_ready')
          .catch(() => false)
          .then((ready) => {
            if (cancelled) return;
            if (ready) {
              document.documentElement.dataset.underlay = '1';
              return;
            }
            tries += 1;
            if (tries < 30) {
              window.setTimeout(check, 100);
              return;
            }
            try {
              sessionStorage.setItem(NATIVE_FAILED_KEY, '1');
            } catch {
              return;
            }
            window.location.reload();
          });
      };
      check();
      return () => {
        cancelled = true;
        delete document.documentElement.dataset.underlay;
      };
    }
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

  // Start every pane and status line store at launch so any package
  // that arrives while a pane is closed (or has not yet been opened)
  // still lands and shows on first open. Imm.Queues especially: the
  // server sends the only guaranteed snapshot at login and has no
  // heartbeat, so a lazily started store would drop it and the pane
  // would sit dark until the next queue change. World.Moons and
  // Room.Info are the same.
  useEffect(() => {
    startStores();
  }, []);

  useEffect(() => {
    // Tauri creates the main window with visible=false so the user
    // doesn't see a default-styled white flash. Reveal once theme and
    // font have applied and a frame with the theme has gone out.
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
        setBaseAnsi(cfg.terminal_base_ansi);
        // This window owns following the OS appearance, so its flips
        // reach the Terminal and every other window.
        applyThemePrefs(cfg, { broadcast: true, broadcastFlips: true });
        setFontFamily(cfg.font_family || DEFAULT_FONT_FAMILY);
        setFontSize(cfg.font_size || 14);
        setTerminalLineHeight(cfg.terminal_line_height);
        setThemeTerminalColors(resolveThemeTerminalColors(cfg.theme, cfg.theme_terminal_colors));
        applyBrightBold(cfg.bright_bold);
        applySplitDividerColor(cfg.split_divider_color);

        // Bring the preset triggers in line with the presets that are
        // on. Take out every preset that is off, or that this build no
        // longer has, and install every one that is on again, so this
        // build's patterns replace older copies. In loadout mode the
        // triggers and the list are shared by every profile, and
        // without the removal a preset you turned off came back after a
        // launch as another character.
        let installed: (string | null | undefined)[] = [];
        try {
          installed = (await listTriggers()).map((t) => t.preset);
        } catch (e) {
          console.error('[presets] listing triggers failed:', e);
        }
        const plan = presetLaunchPlan(cfg.enabled_presets, installed);
        for (const id of plan.remove) {
          try {
            await presetsRemove(id);
          } catch (e) {
            console.error(`[presets] removing ${id} failed:`, e);
          }
        }
        const toInstall = PRESETS.filter((p) => plan.install.includes(p.id)).flatMap(
          presetTriggers,
        );
        if (toInstall.length > 0) {
          try {
            await presetsInstall(toInstall);
          } catch (e) {
            console.error('[presets] startup install failed:', e);
          }
        }
      })
      .catch(() => void applyAndBroadcastTheme('system'))
      .finally(() => showAfterThemePaint(reveal));
    return onUnmount;
  }, []);

  // A profile switch (#profile switch, a Settings click, or the
  // Char.Status swap after login), #profile load, #profile reset, and
  // an import each replace the whole UI config in the backend, while
  // this window still shows the old profile's theme, font, and the
  // rest. Read the new config, apply it here, and send every field to
  // every window, so Input, the vitals, the prompt, and each other
  // per-field listener settle too. The panes, the tracked affects, the
  // tick settings, and the chip style also come from the backend on
  // their own events.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void followReplacedUiConfig(
      (cfg) => {
        if (cancelled) return;
        setCustomThemes((cfg.custom_themes ?? []).map(customToAppTheme));
        setBaseAnsi(cfg.terminal_base_ansi);
        applyThemePrefs(cfg, { broadcast: true });
        setFontFamily(cfg.font_family || DEFAULT_FONT_FAMILY);
        setFontSize(cfg.font_size || 14);
        setTerminalLineHeight(cfg.terminal_line_height);
        setThemeTerminalColors(resolveThemeTerminalColors(cfg.theme, cfg.theme_terminal_colors));
        applyBrightBold(cfg.bright_bold);
        applySplitDividerColor(cfg.split_divider_color);
      },
      (e) => console.error('[app] reading the replaced config failed', e),
      { broadcast: true },
    ).then((fn) => {
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
    loadFontStack(renderFamily);
    root.style.setProperty('--app-font-family', renderFamily);
    root.style.setProperty('--app-font-size', `${fontSize}px`);
    try {
      localStorage.setItem('vosh.cache.fontFamily', fontFamily);
      localStorage.setItem('vosh.cache.fontSize', String(fontSize));
    } catch {
      // cache only; config remains the source of truth
    }
  }, [fontFamily, renderFamily, fontSize]);

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
    try {
      localStorage.setItem('vosh.cache.lineHeight', terminalLineHeight);
    } catch {
      // cache only; config remains the source of truth
    }
  }, [terminalLineHeight]);

  useEffect(() => {
    // Settings save broadcasts the terminal line height.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    subscribeTerminalLineHeightChanged((value) => {
      setTerminalLineHeight(value);
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

  // The native grid draws a band under each lifted prompt while your
  // prompt shows lifted, and under your design while the prompt card
  // draws it in the text (the 2026-09-30 addendum, item 2). xterm keeps
  // its own ground while the card is open, so In the text stays as it
  // is there and the card's marks still show.
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    void invoke('native_surface_set_prompt_bands', {
      on: promptLifted === true || cardBand,
    }).catch(() => {});
  }, [promptLifted, cardBand]);

  // The band under the open row reaches past its last glyph for the
  // prompt card's line break mark and caret.
  const promptReach = usePromptReach();
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    void invoke('native_surface_set_prompt_reach', { px: promptReach }).catch(() => {});
  }, [promptReach]);

  // Keep the native surface's chrome colors on the theme. Every theme
  // apply, from this window, a broadcast, or a profile switch, writes
  // data-theme on the root, so one observer catches them all.
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    pushNativeChromeTokens();
    const observer = new MutationObserver(pushNativeChromeTokens);
    observer.observe(document.documentElement, { attributeFilter: ['data-theme'] });
    return () => observer.disconnect();
  }, []);

  // Under the underlay the DOM owns the pointer over the terminal, but
  // only the native surface knows what sits under it: the split divider
  // wants a resize cursor and an armed link wants a hand. It reports the
  // cursor on each change and the sizer takes it through a variable.
  useEffect(() => {
    if (!nativeUnderlay()) return;
    const root = document.documentElement;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen<string>('vosh://terminal-cursor', (event) => {
      const cursor = event.payload;
      if (cursor === 'row-resize' || cursor === 'pointer') {
        root.style.setProperty('--terminal-cursor', cursor);
      } else {
        root.style.removeProperty('--terminal-cursor');
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
      root.style.removeProperty('--terminal-cursor');
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

  // Right-clicks on the native surface never reach the DOM (the opaque
  // view eats them like left-clicks), so the backend forwards them with
  // the pointer's viewport position and the menu opens from the event.
  // The DOM onContextMenu below covers the xterm renderer.
  useEffect(() => {
    if (!nativeSurfaceEnabled()) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen<[number, number]>('vosh://terminal-context-menu', (event) => {
      const [x, y] = event.payload;
      setTerminalMenu({ x, y });
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
    // Settings saved, or the palette picked, new theme fields. Keep this
    // window's copy current so the OS listener and the next palette pick
    // start from them. The sender already broadcast the resolved theme.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    subscribeThemePrefs((prefs) => {
      applyThemePrefs(prefs);
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
      // The menu bar lists custom themes too.
      setThemeTick((n) => n + 1);
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
    // Base ANSI palette edits from the settings window: update the
    // live override; the Terminal re-derives via its own subscription.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    subscribeBaseAnsiChanged((colors) => {
      setBaseAnsi(colors);
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
    // The game sent a new prompt setting and your capture follows it.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void startGamePromptToasts().then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    // The shared catalog wizard wrote its files. Nothing this session
    // changes saves until Vosh opens again, so say so in the terminal and
    // in a toast that stays up.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    subscribeMigrationApplied(() => {
      showMigrationApplied(writeLive);
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
        // A reason means the link dropped out from under us; a clean
        // user-initiated disconnect carries none and stays quiet.
        if (payload.reason) {
          if (termRef.current) {
            writeLive(`\r\n\x1b[31m[${payload.reason}]\x1b[0m\r\n`);
          }
          pushToast({ kind: 'error', message: 'Connection lost', meta: payload.reason });
        }
      } else {
        setStatus(payload);
        // Push the current terminal size on every (re)connect so the
        // negotiator advertises the live cols × rows via NAWS as soon
        // as the server asks. MUDs that honor NAWS wrap at this width
        // server-side, which is the right answer to word wrap.
        if (payload.kind === 'connected') {
          pushToast({
            kind: 'success',
            message: 'Connected',
            meta: `${payload.host}:${payload.port}`,
          });
          const handle = termRef.current;
          if (handle) {
            const { cols, rows } = handle.windowSize();
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
      preSplitLiveRowsRef.current = termRef.current?.getSize().rows ?? 0;
      setSplitOpen(true);
    } else if (historyTermRef.current && historyReady) {
      if (direction === 'next') historyTermRef.current.findNext(query, opts);
      else historyTermRef.current.findPrevious(query, opts);
    } else {
      pendingFindRef.current = { query, opts, direction };
    }
    return true;
  };

  const connected = status.kind === 'connected' || status.kind === 'connecting';

  const inputElement = (
    <Input
      ref={inputRef}
      enabled={connected}
      promptPinned={promptPinned}
      fontKey={`${fontFamily}|${fontSize}`}
      onError={handleError}
      onSelectAllTerminal={() => termRef.current?.selectAll()}
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
        // Native surface: the grid pages its own display in place
        // (Input invokes native_surface_scroll alongside this), so
        // the DOM split must stay closed — it would be invisible
        // under the opaque surface while still toggling the split
        // layout class.
        if (nativeSurfaceEnabled()) return;
        // Split-scrollback gesture. The live pane (termRef) stays
        // anchored to the tail. PageUp opens the split if closed;
        // the history Terminal mounts on that state change and its
        // onReady does the initial scroll, so we don't touch the
        // ref here (it is null until the mount completes).
        if (pages < 0) {
          if (!splitOpen) {
            // Same pre-split row capture as the wheel path: without it
            // onScrollbackLoaded scrolls back zero rows and the history
            // pane opens showing a duplicate of the live tail.
            preSplitLiveRowsRef.current = termRef.current?.getSize().rows ?? 0;
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

  // Show or hide one pane type from the palette. Showing opens the
  // panel too, and a pane already there under a hidden panel just
  // comes back with it.
  const togglePane = (pane: PaneType) => {
    updatePanelLayout((l) => {
      const leaf = leafIdFor(l.root, pane);
      if (leaf !== null && l.panel_open) return { ...l, root: closePane(l.root, leaf) };
      return { ...l, panel_open: true, root: leaf !== null ? l.root : addPane(l.root, pane) };
    });
  };

  // Add a pane from the title band, at the bottom of the panel.
  const addPaneType = (pane: PaneType) => {
    updatePanelLayout((l) => ({ ...l, panel_open: true, root: addPane(l.root, pane) }));
  };

  // Everything the palette can reach, rebuilt fresh at each open so
  // labels track live state.
  const paletteDeps = (): PaletteDeps => ({
    connected,
    host: status.kind === 'connected' || status.kind === 'connecting' ? status.host : null,
    worldName: connection.world,
    panelOpen,
    togglePanel: togglePanelOpen,
    splitOpen: splitOpen || (nativeSurfaceEnabled() && getNativeScroll().offset > 0),
    toggleSplit,
    // The staff queues row waits for Imm.Queues, like Add a pane, but a
    // pane the tree already shows stays listed so you can hide it.
    paneTypes: PANE_TYPES.filter((t) => offeredPaneTypes().includes(t) || shownPanes.includes(t)),
    paneVisible: (pane) => panelOpen && shownPanes.includes(pane),
    togglePane,
    openHelp: openHelpWindow,
    openFind: () => setFindOpen(true),
    openSettings: openSettingsWindow,
    openSettingsTab,
    connect: () => void connection.connect(),
    disconnect: () => void disconnectSession(),
    insertInput: (text) => inputRef.current?.insert(text),
    promptShow: promptShow?.capture ? promptShow.show : null,
    openPromptCard: (view) => openPromptCard(view === 'text' ? 'text' : 'design'),
    promptDraw: promptShow?.capture ? promptShow.draw : null,
    setPromptDraw: (on) => {
      void promptConfigGet()
        .then((config) => promptConfigSet({ ...config, draw: on }))
        .catch((e: unknown) => pushToast({ kind: 'error', message: String(e) }));
    },
  });

  const closeMainWindow = () => {
    getCurrentWindow()
      .close()
      .catch((e: unknown) => console.error('[main] closing the window failed', e));
  };

  // Edit, then Copy, in the menu bar. With no selection of the page's
  // own, the terminal selection wins, the same as Cmd+C in the command
  // line and Copy in the terminal menu. Otherwise the system copies the
  // field or page selection.
  const copyFromMenu = () => {
    const own = pageHasSelection();
    if (!own && !nativeSurfaceEnabled()) {
      const text = termRef.current?.getSelection() || historyTermRef.current?.getSelection() || '';
      if (text.length > 0) {
        void navigator.clipboard.writeText(text).catch(() => {});
        return;
      }
    }
    menuCopy(!own && nativeSurfaceEnabled());
  };

  // One dispatcher for the window shortcuts and the macOS menu bar, by
  // palette id. It keeps the shortcut gates: a held key repeats only
  // find, and Connect does nothing while a session is live. An id it
  // does not own runs the palette entry of the same id.
  const runCommand = (id: string, opts: { repeat?: boolean } = {}) => {
    const { findOpen: finding, paletteOpen: inPalette, live } = shortcutState.current;
    if (opts.repeat && !commandRepeats(id)) return;
    switch (id) {
      case 'connect':
        if (!live) void connection.connect();
        return;
      case 'panel':
        togglePanelKeepingCaret(() => inputRef.current?.focus());
        return;
      case 'palette':
        setPaletteOpen(!inPalette);
        if (inPalette) inputRef.current?.focus();
        return;
      case 'find':
        // Open just the toolbar. Whether to open the split is decided
        // per search: only when a match would scroll the live pane up
        // off its tail (see submitFind above).
        if (finding) findToolbarRef.current?.focus();
        else setFindOpen(true);
        return;
      case 'settings':
        openSettingsWindow();
        return;
      case 'help':
        openHelpWindow();
        return;
      case 'split':
        toggleSplit();
        return;
      case 'session-edit':
        requestSessionMenu('edit');
        return;
      case 'session-new':
        requestSessionMenu('new');
        return;
      case 'close-window':
        if (live) setConfirmClose(true);
        else closeMainWindow();
        return;
      case 'copy':
        copyFromMenu();
        return;
    }
    const entry = [...buildPaletteEntries(paletteDeps()), ...themeEntries()].find(
      (e) => e.id === id,
    );
    if (entry) void entry.run();
  };
  useEffect(() => {
    shortcutState.current = { findOpen, paletteOpen, live: connection.live };
    runCommandRef.current = runCommand;
  });

  // The macOS menu bar mirrors this window. Theme applies write
  // data-theme on the root, so one observer catches every source.
  useEffect(() => {
    if (!isMacPlatform()) return;
    const observer = new MutationObserver(() => setThemeTick((n) => n + 1));
    observer.observe(document.documentElement, { attributeFilter: ['data-theme'] });
    setStaffOffered(getImmState().received);
    const unsubImm = subscribeImmState((s) => setStaffOffered(s.received));
    let unsubScroll = () => {};
    if (nativeSurfaceEnabled()) {
      startNativeScroll();
      const readScroll = () => setNativeScrolled(getNativeScroll().offset > 0);
      readScroll();
      unsubScroll = subscribeNativeScroll(readScroll);
    }
    return () => {
      observer.disconnect();
      unsubImm();
      unsubScroll();
    };
  }, []);

  // Send the menu bar a snapshot when one of its inputs changes. The
  // theme tick stands in for the theme id and the custom theme list.
  useEffect(() => {
    if (!isMacPlatform()) return;
    setAppMenuState(
      buildMenuState({
        live: connection.live,
        worldName: connection.world,
        panelOpen,
        splitOpen: splitOpen || nativeScrolled,
        shownPanes,
        staffOffered,
        themes: themesInGalleryOrder().map(({ theme, custom }) => ({
          id: theme.id,
          label: theme.label,
          custom,
        })),
        theme: getCurrentThemeId(),
      }),
    );
  }, [
    connection.live,
    connection.world,
    panelOpen,
    splitOpen,
    nativeScrolled,
    shownPanes,
    staffOffered,
    themeTick,
  ]);

  const terminalAreaElement = (
    <div
      ref={terminalAreaRef}
      className={`terminal-area${splitOpen ? ' terminal-area-split' : ''}${
        findOpen && nativeSurfaceEnabled() && !nativeUnderlay() ? ' terminal-area-find-inset' : ''
      }`}
      onMouseUp={handleTerminalMouseUp}
      onContextMenu={(event) => {
        // Replace the webview's default context menu with ours.
        event.preventDefault();
        setTerminalMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      {findOpen && (
        <FindToolbar
          ref={findToolbarRef}
          results={findResults}
          onFindNext={(query, opts) => submitFind(query, opts, 'next')}
          onFindPrevious={(query, opts) => submitFind(query, opts, 'previous')}
          onClose={closeFind}
        />
      )}
      <ScrollDepth findOpen={findOpen} />
      {/* The containing block for the scrollback split, where the well
          splits wrapper was. It never changes, so opening the split or
          toggling the panel never remounts the live Terminal. */}
      <div className="terminal-well">
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
            defaultSize={240}
            minSize={80}
            maxSize={1200}
            reservePx={120}
            className={`terminal-pane terminal-pane-history${historyReady ? '' : ' terminal-pane-history-priming'}`}
            handleLabel="resize scrollback split"
            snapPx={() => termRef.current?.cellHeight() ?? 0}
          >
            <Terminal
              fontFamily={renderFamily}
              fontSize={fontSize}
              lineHeight={TERMINAL_LINE_HEIGHTS[terminalLineHeight]}
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
            fontFamily={renderFamily}
            fontSize={fontSize}
            lineHeight={TERMINAL_LINE_HEIGHTS[terminalLineHeight]}
            themeTerminalColors={themeTerminalColors}
            onReady={(handle) => {
              termRef.current = handle;
            }}
            // After the restored scrollback, so what launch has to tell
            // you lands below it instead of scrolling away above.
            onScrollbackLoaded={() => void showLaunchNotices(writeLive)}
            onResultsChanged={(event) =>
              setFindResults({ index: event.resultIndex, count: event.resultCount })
            }
            onCellSize={setCellSize}
            lifted={promptLifted}
            lentRows={dockLent}
            anchorBottom={dockShows}
          />
        </div>
      </div>
      {/* Your prompt pinned above the command line. It takes one row from
          the terminal only while your prompt shows pinned, and borrows
          the rows past its first from the bottom of the live pane. */}
      {promptPinned && promptShow && cellSize && (
        <PromptDock
          state={promptShow}
          cell={cellSize}
          fontSize={fontSize}
          themeTerminalColors={themeTerminalColors}
          brightBold={brightBold}
          renderer={nativeSurfaceEnabled() ? 'native' : 'xterm'}
        />
      )}
    </div>
  );

  // The shell gives each slot a fixed grid cell under a fixed parent,
  // and showing, hiding, or resizing the panel only rewrites the column
  // template on the shell root. terminalAreaElement and inputElement
  // therefore keep their parents for the life of the window, so the
  // live Terminal and the Input never remount: no scrollback reload, no
  // lost underlay pointer forwarding, and no dropped command history.
  // The panel keeps its panes mounted while hidden, too.
  return (
    <AppShell
      panelOpen={panelOpen}
      panelWidth={panelWidth}
      onPanelWidth={setPanelWidth}
      onMouseUp={handleAppMouseUp}
      titleBand={
        <TitleBand
          connection={connection}
          panelOpen={panelOpen}
          onTogglePanel={() => togglePanelKeepingCaret(() => inputRef.current?.focus())}
          onTogglePalette={() => (paletteOpen ? closePalette() : setPaletteOpen(true))}
          paneTree={panelLayout?.root ?? null}
          onAddPane={addPaneType}
          onMenuClosed={() => inputRef.current?.focus()}
        />
      }
      terminal={terminalAreaElement}
      input={inputElement}
      statusLine={<StatusLine connected={connection.live} showVitals={!panelOpen} />}
      panel={<PanelHost promptShow={promptShow} />}
    >
      <UpdateNotice />
      <Toasts />
      {terminalMenu && (
        <TerminalMenu
          x={terminalMenu.x}
          y={terminalMenu.y}
          termRef={termRef}
          inputRef={inputRef}
          onOpenFind={() => setFindOpen(true)}
          onCustomizePrompt={() => openPromptCard('design')}
          onClose={() => setTerminalMenu(null)}
        />
      )}
      {promptCard && (
        <PromptCard
          opening={promptCard}
          onBand={setCardBand}
          host={promptCardHost}
          show={promptShow}
          cell={cellSize}
          monoFamily={renderFamily}
          themeTerminalColors={themeTerminalColors}
          brightBold={brightBold}
          renderer={nativeSurfaceEnabled() ? 'native' : 'xterm'}
          onClose={closePromptCard}
        />
      )}
      {paletteOpen && <CommandPalette deps={paletteDeps()} onClose={closePalette} />}
      {confirmClose && (
        <ConfirmDialog
          title="Close this window?"
          body={`You are connected to ${connection.world}. Closing this window ends your session and quits Vosh.`}
          confirmLabel="Close window"
          onConfirm={() => {
            setConfirmClose(false);
            closeMainWindow();
          }}
          onCancel={() => setConfirmClose(false)}
        />
      )}
    </AppShell>
  );
}

export default App;
