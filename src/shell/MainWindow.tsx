import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Terminal } from '../terminal/Terminal';
import type { TerminalHandle } from '../terminal/terminalHandle';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { Input, type InputHandle } from '../input/Input';
import { useMacroKeys } from '../input/useMacroKeys';
import { Resizable } from '../terminal/Resizable';
import { ReconnectNotice } from './overlays/ReconnectNotice';
import { CornerNotices } from './overlays/CornerNotices';
import { FindToolbar } from '../terminal/FindToolbar';
import { TerminalMenu } from '../terminal/TerminalMenu';
import { ScrollDepth } from '../terminal/ScrollDepth';
import { AppShell } from './AppShell';
import { GetStarted } from './getStarted/GetStarted';
import { markDone, openList as openGetStarted } from './getStarted/getStartedStore';
import { showMe } from './getStarted/showMe';
import type { StepId } from './getStarted/steps';
import { openNewSession } from './newSession';
import { SessionSidebar, type SessionSidebarHandle } from './SessionSidebar';
import { SessionsToggle } from './SessionsToggle';
import { SnoopSplit } from './SnoopSplit';
import { requestSnoop } from './snoopKeys';
import { TitleBand } from './TitleBand';
import { StatusLine } from './StatusLine';
import { PanelHost } from '../panel/PanelHost';
import {
  panelWidthOf,
  setPanelOpen,
  setPanelWidth,
  togglePanelOpen,
  usePanelLayout,
} from '../panel/panelLayoutStore';
import { addPaneAtBottom, togglePane } from '../panel/paneActions';
import {
  promptConfigGet,
  promptConfigSet,
  promptCodeReaderSet,
  subscribePromptCardOpen,
} from '../ipc/prompt';
import { promptPreviewSet } from '../ipc/promptDesign';
import { disconnectSession } from '../ipc/session';
import { snoopClose, snoopStop, snoopWindowOpen } from '../ipc/snoop';
import { TERMINAL_LINE_HEIGHTS } from '../ipc/uiConfig';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { openHelpWindow, openSettingsWindow } from '../ipc/windows';
import { subscribeMigrationApplied } from '../ipc/wizard';
import { listenForQuitFlush } from '../lib/pendingWrites';
import { startStores } from '../stores';
import { pushToast } from '../stores/toasts';
import { showMigrationApplied } from './launchNotices';
import { startGamePromptToasts } from '../prompt/gamePromptToast';
import { CommandPalette } from './overlays/CommandPalette';
import type { PaletteDeps } from './overlays/palette';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { CoachRing } from '../ui/CoachRing';
import { openSettingsTab } from '../lib/settingsLink';
import { requestSessionMenu } from '../lib/appMenu';
import { getNativeScroll } from '../terminal/native/nativeScroll';
import { allPanes, PANE_TYPES } from '../panel/paneLayout';
import { offeredPaneTypes } from '../panel/paneTypes';
import {
  getSelected,
  goTo,
  move,
  rename,
  select,
  sessionStep,
  useOpened,
  useSelected,
  useSessions,
} from '../stores/session/sessionsStore';
import { getSnoops } from '../stores/session/snoopStore';
import { useConnection } from '../stores/session/useConnection';
import { useVitalsOptions } from '../stores/config/vitalsOptionsStore';
import { useEscape } from '../lib/escapeStack';
import { usePromptShow } from '../prompt/showState';
import { PromptDock } from '../prompt/PromptDock';
import { PROMPT_BINDING, VITALS_TEXT_BINDING, type CardBinding } from '../prompt/cardBinding';
import { PromptCard, type PromptCardHost } from '../prompt/PromptCard';
import { nextCardRequest, type CardRequest, type CardRequestView } from '../prompt/cardRules';
import { usePinnedDockRows } from '../stores/session/pinnedPromptStore';
import { lentRows, type CellSize } from '../prompt/pinnedDock';
import { WritingCard, type WritingRequest } from '../writing/WritingCard';
import { WritingOffer } from '../writing/WritingOffer';
import { hasBeast, writable } from '../writing/kinds';
import type { WritingKind } from '../ipc/writing';
import { useCharStatus } from '../stores/gmcp/charStatusStore';
import { useAlertTones } from './useAlertTones';
import { useAppCommands } from './useAppCommands';
import { useClosing } from './useClosing';
import { useFind } from './useFind';
import { useNativeSurfaceBridge } from './useNativeSurfaceBridge';
import { useScrollbackSplit } from './useScrollbackSplit';
import { useSessionTerminals } from './useSessionTerminals';
import { useSessionsSidebar } from './useSessionsSidebar';
import { useUiConfigFollow } from './useUiConfigFollow';

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

function MainWindow() {
  // The panel's open state, width, and pane tree, per profile. The
  // panel's layout store is the one copy in this window. The title
  // band and the palette show, hide, and size the panel through it,
  // and the panel edits the tree through it.
  const panelLayout = usePanelLayout();
  // Where your prompt shows, and whether this profile reads one.
  const promptShow = usePromptShow();
  const promptPinned = promptShow?.show === 'pinned' && promptShow.capture;
  const promptLifted = promptShow?.show === 'lifted' && promptShow.capture;
  // The session the window shows, whose prompt the card works on.
  const selected = useSelected();
  // The sessions this window opened. Each keeps a live terminal of its
  // own until it closes, and only the selected session's shows.
  const opened = useOpened();
  // Every open session, which the sessions sidebar lists. It shows while
  // two or more are open, until the sessions toggle hides it for this
  // window (Q17) or the window grows too narrow to hold it (board 8),
  // where the toggle slides it over the terminal instead (Sessions
  // toggle T5).
  const sessions = useSessions();
  const panelOpen = panelLayout?.panel_open ?? true;
  // The status line carries your vitals with the panel hidden, or with
  // Show your vitals in on Status line.
  const vitalsPlace = useVitalsOptions().place;
  const lineShowsVitals = !panelOpen || vitalsPlace === 'status';
  // The sidebar hands the caret back to the command line as it goes.
  const sessionsSidebar = useSessionsSidebar(sessions.length, panelOpen, () =>
    inputRef.current?.focus(),
  );
  const sessionsShown = sessionsSidebar.shown;
  // Rename session… names the selected session in its row while the
  // sidebar shows (Q7), and in the session popover's own form while it
  // does not, as with one session.
  const sidebar = useRef<SessionSidebarHandle | null>(null);
  const renameSession = () => {
    if (sidebar.current) sidebar.current.rename(getSelected());
    else requestSessionMenu({ mode: 'rename' });
  };
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
  const dockShows = promptPinned && cellSize !== null && dockShown > 0;
  const dockLent = dockShows ? lentRows(dockShown) : 0;
  const panelWidth = panelWidthOf(panelLayout);
  const shownPanes = useMemo(() => (panelLayout ? allPanes(panelLayout.root) : []), [panelLayout]);
  // Each opened session's live terminal, the selected session's, which
  // the find bar, the split, the menus and the prompt card reach, and
  // what the window writes into them.
  const { termRef, writeTo, writeLive, handleError, onTerminalReady, onScrollbackLoaded } =
    useSessionTerminals(selected, opened);
  const historyTermRef = useRef<TerminalHandle | null>(null);
  const inputRef = useRef<InputHandle | null>(null);
  // The selected session's macros, which the command line fires and the
  // session keys ask after (Q11).
  const macroKeys = useMacroKeys();
  // Puts the caret back on the command line.
  const focusInput = () => inputRef.current?.focus();
  // The selected session for the title band, the session menu, the
  // palette, and Cmd+R.
  const connection = useConnection(handleError);
  // Direct ref on the terminal-area wrapper so we can attach a
  // non-passive wheel listener. JSX onWheel is passive in some
  // React versions and silently no-ops preventDefault, which would
  // let xterm scroll the live pane underneath us.
  const terminalAreaRef = useRef<HTMLDivElement | null>(null);
  // ⌘K command palette.
  const [paletteOpen, setPaletteOpen] = useState(false);
  // Right-click context menu over the terminal area. Non-null while
  // open; the value is the pointer's viewport position (the menu
  // clamps itself to the window edges).
  const [terminalMenu, setTerminalMenu] = useState<{ x: number; y: number } | null>(null);
  // The prompt card (Customize prompt…), open over your prompt, and the
  // view it opens on, or `point` to open on pointing at your game's line.
  const [promptCard, setPromptCard] = useState<CardRequest | null>(null);
  // What the card edits: your prompt, or your vitals text.
  const [cardBinding, setCardBinding] = useState<CardBinding>(PROMPT_BINDING);
  // The writing card, open over the terminal on a kind of text, or on
  // the offer the game's editor brought. It and the prompt card share the
  // place over your prompt, so one opening closes the other.
  const [writingCard, setWritingCard] = useState<WritingRequest | null>(null);
  // Every request counts, so the open card hears a repeat of one.
  const openPromptCard = useCallback(
    (view: CardRequestView, binding: CardBinding = PROMPT_BINDING) => {
      setWritingCard(null);
      setCardBinding(binding);
      setPromptCard((prev) => nextCardRequest(prev, view));
    },
    [],
  );
  const openWriting = useCallback((kind: WritingKind, offer?: number) => {
    setPromptCard(null);
    setWritingCard((prev) => ({
      kind,
      n: (prev?.n ?? 0) + 1,
      ...(offer !== undefined ? { offer } : {}),
    }));
  }, []);
  // Your race and level, which decide the boards you write on and a
  // werebeast's beast.
  const charStatus = useCharStatus();
  const writeKinds = writable(charStatus.level);
  // The card draws your design over the band of Lifted in the text.
  const [cardBand, setCardBand] = useState(false);

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

  // The prompt card reaches the terminal it sits over through these. The
  // ref never changes, so the host never does.
  const promptCardHost = useMemo<PromptCardHost>(
    () => ({
      terminal: () => termRef.current,
      area: () => terminalAreaRef.current,
      dock: () => document.querySelector<HTMLElement>('.prompt-dock'),
    }),
    [termRef],
  );
  const closePromptCard = () => {
    setPromptCard(null);
    setCardBand(false);
    focusInput();
  };
  const closeWriting = () => {
    setWritingCard(null);
    focusInput();
  };

  // Customize… in Settings, and anything else in another window, opens
  // the card here and brings this window forward. Edit… under Customize
  // vitals and Edit your text… on the vitals menu open it on your
  // vitals text.
  useTauriEvent(subscribePromptCardOpen, (request) => {
    openPromptCard(request.view ?? 'design', request.vitals ? VITALS_TEXT_BINDING : PROMPT_BINDING);
    void getCurrentWindow()
      .setFocus()
      .catch(() => {});
  });

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

  // The scrollback split on xterm: the history pane, the wheel and keys
  // that open and page it, and the find match it shows from scrollback.
  const {
    splitOpen,
    historyReady,
    historyScrollPos,
    toggleSplit,
    middleClick,
    pageSplit,
    exitSplit,
    onHistoryLoaded,
    onHistoryScroll,
    showHistoryMatch,
    hideHistoryMatch,
    clearQueuedSearch,
  } = useScrollbackSplit({
    session: selected,
    termRef,
    historyTermRef,
    terminalAreaRef,
    focusInput,
  });

  // Showing, hiding, or resizing the panel or the sessions sidebar
  // changes the terminal column's width. FitAddon's own internal observers do not always
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
  }, [termRef, panelOpen, panelWidth, sessionsShown, sessionsSidebar.width]);

  // Click anywhere in the terminal area focuses the input. Skip when
  // the user is selecting text (so copy still works) or clicking an
  // actual interactive element. A middle click goes to the split.
  const handleTerminalMouseUp = (event: MouseEvent<HTMLDivElement>) => {
    if (event.button === 1) {
      middleClick(event);
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
    focusInput();
  };

  // Tauri reports a window-level focus event when the OS brings the
  // app back to front (user clicked the Vosh window while it was
  // unfocused, or alt-tabbed in). Focusing the input here is the
  // "click-to-type" affordance the user expects on every reactivation.
  useEffect(() => {
    window.addEventListener('focus', focusInput);
    return () => window.removeEventListener('focus', focusInput);
  }, []);

  // Mark the root while the window is in the background, so frame.css
  // can dim the window title to the tertiary tone the way the OS dims
  // an inactive title bar.
  useEffect(() => {
    const root = document.documentElement;
    const mark = () => {
      root.dataset.windowFocus = document.hasFocus() ? 'focused' : 'unfocused';
    };
    mark();
    window.addEventListener('focus', mark);
    window.addEventListener('blur', mark);
    return () => {
      window.removeEventListener('focus', mark);
      window.removeEventListener('blur', mark);
      delete root.dataset.windowFocus;
    };
  }, []);

  // After a copy the caret should land back on the command line. The
  // terminal copy path dispatches `vosh:focus-input` explicitly; the
  // DOM `copy` listener is the catch-all for a browser-native copy of
  // selected terminal text. Both skip when the copy came from a field
  // (the command input, the find box) so that field keeps its focus,
  // and the copy listener defers a frame so the clipboard reads the
  // selection before focus moves off it.
  useEffect(() => {
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

  // Find in scrollback. A match up in scrollback shows in the split.
  const { findOpen, openFind, findToolbarRef, finds, closeFind, submitFind, onFindResults } =
    useFind({
      session: selected,
      termRef,
      historyTermRef,
      focusInput,
      showHistoryMatch,
      hideHistoryMatch,
      clearQueuedSearch,
    });

  const closePalette = () => {
    setPaletteOpen(false);
    focusInput();
  };

  // Esc closes the open surface on top and nothing under it (see
  // lib/escapeStack). useFind above registers the find bar first. The
  // palette handles Esc inside itself, where it first steps back out of
  // a submenu.
  useEscape(paletteOpen, closePalette, () => document.querySelector('.ov-palette'));
  useEscape(terminalMenu !== null, () => {
    setTerminalMenu(null);
    focusInput();
  });

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

  const { status, live: connected } = connection;

  // Closing a session, this window or the app, and the question each
  // asks while sessions are connected.
  const closing = useClosing();

  // A snoop row's call to the game, which says in a toast when it fails.
  const snoopCall = (call: Promise<void>) =>
    void call.catch((e: unknown) => pushToast({ kind: 'error', message: String(e) }));

  // Everything the palette can reach, rebuilt fresh at each open so
  // labels track live state.
  const paletteDeps = (): PaletteDeps => ({
    connected,
    redialing: connection.redialing,
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
    openGetStarted,
    openFind,
    openSettings: openSettingsWindow,
    openSettingsTab,
    connect: () => void connection.connect(),
    newSession: () => void openNewSession(),
    renameSession,
    closeSession: () => closing.closeSession(),
    sessions: {
      rows: sessions,
      selected,
      shown: sessionsSidebar.pressed,
      goTo,
      step: (step) => goTo(sessionStep(step)),
      toggleShown: sessionsSidebar.toggle,
    },
    snoops: {
      tabs: getSnoops().tabs,
      goTo: () => requestSnoop('enter'),
      next: () => requestSnoop('next'),
      stop: (name) => snoopCall(snoopStop(getSelected(), name)),
      openWindow: () => snoopCall(snoopWindowOpen(getSelected())),
      closeEnded: () => snoopCall(snoopClose(getSelected())),
    },
    disconnect: () => void disconnectSession(getSelected()),
    insertInput: (text) => inputRef.current?.insert(text),
    promptShow: promptShow?.capture ? promptShow.show : null,
    openPromptCard: (view) => openPromptCard(view === 'text' ? 'text' : 'design'),
    writing: {
      kinds: writeKinds,
      beast: hasBeast(charStatus.race, charStatus.level),
      open: (kind) => openWriting(kind),
    },
    promptDraw: promptShow?.capture ? promptShow.draw : null,
    setPromptDraw: (on) => {
      const session = getSelected();
      void promptConfigGet(session)
        .then((config) => promptConfigSet({ ...config, draw: on }, { session }))
        .catch((e: unknown) => pushToast({ kind: 'error', message: String(e) }));
    },
  });

  // Show me opens the panel or the terminal menu here (showMe.ts).
  const showMeHere = (step: StepId) =>
    showMe(step, {
      openPanel: () => setPanelOpen(true),
      openTerminalMenu: () => {
        const area = terminalAreaRef.current?.getBoundingClientRect();
        const term = termRef.current;
        if (!area) return;
        const last = term ? term.rowTop(term.getSize().rows - 1) : null;
        setTerminalMenu({ x: area.left + 12, y: last ?? area.bottom - 24 });
      },
    });

  // The window shortcuts, the macOS menu bar and #help.
  const { runCommand, themesChanged } = useAppCommands({
    connection,
    macroKeys,
    closeSession: () => closing.closeSession(),
    closeWindow: closing.closeWindow,
    quit: closing.quit,
    splitOpen,
    toggleSplit,
    findOpen,
    openFind,
    findToolbarRef,
    paletteOpen,
    setPaletteOpen,
    togglePanel: () => togglePanelKeepingCaret(focusInput),
    focusInput,
    panelOpen,
    shownPanes,
    sessionCount: sessions.length,
    sessionsShown: sessionsSidebar.pressed,
    toggleSessions: sessionsSidebar.toggle,
    termRef,
    historyTermRef,
    writeLive,
    paletteDeps,
  });

  // The fonts, the sizes and the terminal settings, read at launch and
  // kept up with every Settings change and profile switch. The menu bar
  // lists custom themes too, so a change to them ticks it.
  const {
    fontFamily,
    renderFamily,
    fontSize,
    panelTextPx,
    terminalLineHeight,
    themeTerminalColors,
    brightBold,
    blinkText,
  } = useUiConfigFollow({ onThemesChanged: themesChanged });
  // The terminal settings the Text style of your vitals draws with, in
  // the footer or the status line.
  const textColors = useMemo(
    () => ({ themeTerminalColors, brightBold }),
    [themeTerminalColors, brightBold],
  );

  // Keep the native surface under the page in step with this window.
  useNativeSurfaceBridge({
    session: selected,
    blinkText,
    promptLifted,
    cardBand,
    focusInput,
  });

  // Play the tone of each alert a session rings.
  useAlertTones();

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

  // The shared catalog wizard wrote its files. Nothing this session
  // changes saves until Vosh opens again, so say so in the terminal and
  // in a toast that stays up.
  useTauriEvent(subscribeMigrationApplied, () => {
    showMigrationApplied(writeLive);
  });

  const inputElement = (
    <Input
      ref={inputRef}
      enabled={connected}
      macroKeys={macroKeys}
      fontKey={`${fontFamily}|${fontSize}`}
      onError={handleError}
      onSelectAllTerminal={() => termRef.current?.selectAll()}
      onLocalEcho={(text, session) => {
        writeTo(session, text);
        // Mirror to the split history pane so typed lines appear
        // there too. Without this, scrolling up in split view
        // shows server output but none of your own commands. The
        // history Terminal is lazy-mounted and may be null between
        // splitOpen=true and onReady; the optional chain absorbs
        // that gap. It shows the selected session's history.
        if (session === getSelected()) historyTermRef.current?.write(text);
      }}
      onScrollTerminal={pageSplit}
      onExitSplit={exitSplit}
    />
  );

  const terminalAreaElement = (
    <div
      ref={terminalAreaRef}
      className={`terminal-area${splitOpen ? ' terminal-area-split' : ''}`}
      onMouseUp={handleTerminalMouseUp}
      onContextMenu={(event) => {
        // Replace the webview's default context menu with ours.
        event.preventDefault();
        setTerminalMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      {/* Each opened session with its find bar open keeps it, with its
          query, and the selected session's shows. */}
      {[...finds]
        .filter(([id]) => opened.includes(id))
        .map(([id, results]) => (
          <FindToolbar
            key={id}
            ref={id === selected ? findToolbarRef : undefined}
            hidden={id !== selected}
            results={results}
            onFindNext={(query, opts) => submitFind(query, opts, 'next')}
            onFindPrevious={(query, opts) => submitFind(query, opts, 'previous')}
            onClose={closeFind}
          />
        ))}
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
              key={selected}
              session={selected}
              fontFamily={renderFamily}
              fontSize={fontSize}
              lineHeight={TERMINAL_LINE_HEIGHTS[terminalLineHeight]}
              themeTerminalColors={themeTerminalColors}
              blinkText={blinkText}
              quiet
              onReady={(handle) => {
                historyTermRef.current = handle;
              }}
              onScrollbackLoaded={onHistoryLoaded}
              onScrollPosition={onHistoryScroll}
            />
            {historyScrollPos && historyScrollPos.max > 0 && (
              <div className="scrollback-indicator" aria-live="polite">
                ↑ {historyScrollPos.back} / {historyScrollPos.max}
              </div>
            )}
          </Resizable>
        )}
        {/* One live terminal for each session this window opened, keyed
            by session, so a selection only shows one and hides another.
            A session launch restored opens as its first selection
            finishes, and its scrollback loads then. */}
        <div className="terminal-pane terminal-pane-live">
          {opened.map((id) => (
            <Terminal
              key={id}
              session={id}
              shown={id === selected}
              fontFamily={renderFamily}
              fontSize={fontSize}
              lineHeight={TERMINAL_LINE_HEIGHTS[terminalLineHeight]}
              themeTerminalColors={themeTerminalColors}
              blinkText={blinkText}
              onReady={onTerminalReady(id)}
              onScrollbackLoaded={onScrollbackLoaded(id)}
              onResultsChanged={(event) => onFindResults(id, event)}
              onCellSize={setCellSize}
              lifted={promptLifted}
              lentRows={dockLent}
              anchorBottom={dockShows}
            />
          ))}
        </div>
      </div>
      {/* Your prompt pinned above the command line. It takes one row from
          the terminal only while your prompt shows pinned and a prompt
          is pinned, and borrows the rows past its first from the bottom
          of the live pane. */}
      {promptPinned && promptShow && cellSize && (
        <PromptDock
          state={promptShow}
          cell={cellSize}
          fontSize={fontSize}
          themeTerminalColors={themeTerminalColors}
          brightBold={brightBold}
          renderer={nativeSurfaceEnabled() ? 'native' : 'xterm'}
          blinkText={blinkText}
        />
      )}
    </div>
  );

  // The sessions sidebar, in its column or over the terminal. Over the
  // terminal it takes the keyboard as it slides in, and picking a row or
  // opening a session puts it away.
  const sessionSidebar = (over: boolean) => (
    <SessionSidebar
      ref={sidebar}
      rows={sessions}
      selected={selected}
      onSelect={(session) => {
        if (over) sessionsSidebar.closeOverlay();
        void select(session);
      }}
      onNewSession={() => {
        if (over) sessionsSidebar.closeOverlay();
        void openNewSession();
      }}
      onClose={closing.closeSession}
      takeFocus={over}
      onCaret={focusInput}
      onRename={(session, name) => void rename(session, name)}
      onEditConnection={() => requestSessionMenu({ mode: 'edit' })}
      onDisconnect={(session) =>
        void disconnectSession(session).catch((e: unknown) => handleError(String(e), session))
      }
      onMove={(session, to) => void move(session, to)}
    />
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
      sessionsWidth={sessionsSidebar.width}
      onSessionsWidth={sessionsSidebar.setWidth}
      sessions={sessionsShown ? sessionSidebar(false) : null}
      sessionsOverlay={sessionsSidebar.overlay ? sessionSidebar(true) : null}
      sessionsToggle={
        sessionsSidebar.toggleable ? (
          <SessionsToggle pressed={sessionsSidebar.pressed} onToggle={sessionsSidebar.toggle} />
        ) : null
      }
      titleBand={
        <TitleBand
          connection={connection}
          panelOpen={panelOpen}
          onTogglePanel={() => togglePanelKeepingCaret(focusInput)}
          onTogglePalette={() => (paletteOpen ? closePalette() : setPaletteOpen(true))}
          onOpenSettings={() => runCommand('settings')}
          paneTree={panelLayout?.root ?? null}
          onAddPane={addPaneAtBottom}
          onMenuClosed={focusInput}
          renameInRow={sessionsShown ? () => sidebar.current?.rename(getSelected()) : undefined}
          listSessions={sessionsSidebar.folded}
          onCloseSession={closing.closeSession}
        />
      }
      snoop={
        <SnoopSplit
          session={selected}
          fontFamily={renderFamily}
          fontSize={fontSize}
          lineHeight={TERMINAL_LINE_HEIGHTS[terminalLineHeight]}
          themeTerminalColors={themeTerminalColors}
          onCaret={focusInput}
        />
      }
      terminal={terminalAreaElement}
      input={inputElement}
      statusLine={
        <StatusLine
          connected={connection.live}
          showVitals={lineShowsVitals}
          textColors={textColors}
        />
      }
      panel={<PanelHost promptShow={promptShow} textSize={panelTextPx} textColors={textColors} />}
    >
      <CornerNotices
        reconnect={
          <ReconnectNotice
            session={selected}
            onTryAgain={() => void connection.connect()}
            onError={handleError}
          />
        }
        offer={<WritingOffer onOpen={openWriting} />}
      />
      <GetStarted
        play={{
          live: connected,
          character: connection.character,
          connect: () => void connection.connect(),
        }}
        covered={promptCard !== null}
        host={promptCardHost}
        cell={cellSize}
        show={promptShow}
        onShowMe={showMeHere}
        focusInput={focusInput}
      />
      {terminalMenu && (
        <TerminalMenu
          x={terminalMenu.x}
          y={terminalMenu.y}
          session={selected}
          termRef={termRef}
          inputRef={inputRef}
          onOpenFind={openFind}
          onCustomizePrompt={() => openPromptCard('design')}
          writeKinds={writeKinds}
          onWrite={(kind) => openWriting(kind)}
          onClose={() => setTerminalMenu(null)}
        />
      )}
      <CoachRing />
      {promptCard && (
        // A selection mounts the card again for the session it brings to
        // the front, and the card it leaves puts that session's live
        // prompt back as it goes. Turning from your prompt to your vitals
        // text mounts it again too.
        <PromptCard
          key={`${selected}:${cardBinding.kind}`}
          session={selected}
          binding={cardBinding}
          opening={promptCard}
          onBand={setCardBand}
          host={promptCardHost}
          show={promptShow}
          cell={cellSize}
          monoFamily={renderFamily}
          themeTerminalColors={themeTerminalColors}
          brightBold={brightBold}
          renderer={nativeSurfaceEnabled() ? 'native' : 'xterm'}
          onPromptDone={() => markDone('prompt')}
          onClose={closePromptCard}
        />
      )}
      {writingCard && (
        // A selection mounts the card again for the session it brings to
        // the front.
        <WritingCard
          key={selected}
          session={selected}
          request={writingCard}
          host={promptCardHost}
          cell={cellSize}
          fontFamily={renderFamily}
          fontSize={fontSize}
          themeTerminalColors={themeTerminalColors}
          brightBold={brightBold}
          renderer={nativeSurfaceEnabled() ? 'native' : 'xterm'}
          onClose={closeWriting}
        />
      )}
      {paletteOpen && <CommandPalette deps={paletteDeps()} onClose={closePalette} />}
      {closing.asking && (
        <ConfirmDialog
          title={closing.asking.title}
          body={closing.asking.body}
          confirmLabel={closing.asking.confirm}
          onConfirm={closing.asking.onConfirm}
          onCancel={closing.cancel}
        />
      )}
    </AppShell>
  );
}

export default MainWindow;
