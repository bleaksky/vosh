import { useEffect, useMemo, useRef, useState } from 'react';
import type { TerminalHandle } from '../terminal/terminalHandle';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { Input, type InputHandle } from '../input/Input';
import { useMacroKeys } from '../input/useMacroKeys';
import { ReconnectNotice } from './overlays/ReconnectNotice';
import { CornerNotices } from './overlays/CornerNotices';
import { TerminalMenu } from '../terminal/TerminalMenu';
import { ScreenReaderFeed } from '../terminal/ScreenReaderFeed';
import { AppShell } from './AppShell';
import { GetStarted } from './getStarted/GetStarted';
import { markDone } from './getStarted/getStartedStore';
import { showMe } from './getStarted/showMe';
import type { StepId } from './getStarted/steps';
import { openNewSession } from './newSession';
import { SessionSidebar, type SessionSidebarHandle } from './SessionSidebar';
import { SessionsToggle } from './SessionsToggle';
import { SnoopSplit } from './SnoopSplit';
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
import { addPaneAtBottom } from '../panel/paneActions';
import { disconnectSession } from '../ipc/session';
import { TERMINAL_LINE_HEIGHTS } from '../ipc/uiConfig';
import { listenForQuitFlush } from '../lib/pendingWrites';
import { startStores } from '../stores';
import { CommandPalette } from './overlays/CommandPalette';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { CoachRing } from '../ui/CoachRing';
import { requestSessionMenu } from '../lib/appMenu';
import { allPanes } from '../panel/paneLayout';
import {
  getSelected,
  move,
  rename,
  select,
  useOpened,
  useSelected,
  useSessions,
} from '../stores/session/sessionsStore';
import { useConnection } from '../stores/session/useConnection';
import { useVitalsOptions } from '../stores/config/vitalsOptionsStore';
import { useScreenReader } from '../stores/config/screenReaderStore';
import { useEscape } from '../lib/escapeStack';
import { usePromptShow } from '../prompt/showState';
import { PromptCard } from '../prompt/PromptCard';
import { usePinnedDockRows } from '../stores/session/pinnedPromptStore';
import { lentRows, type CellSize } from '../prompt/pinnedDock';
import { WritingCard } from '../writing/WritingCard';
import { WritingOffer } from '../writing/WritingOffer';
import { terminalArea } from './terminalArea';
import { useAppCommands } from './useAppCommands';
import { useCardRequests } from './useCardRequests';
import { paletteDepsFor } from './paletteDeps';
import { useClosing } from './useClosing';
import { useFind } from './useFind';
import { useNativeSurfaceBridge } from './useNativeSurfaceBridge';
import { useScrollbackSplit } from './useScrollbackSplit';
import { useSessionTerminals } from './useSessionTerminals';
import { useSessionsSidebar } from './useSessionsSidebar';
import { useUiConfigFollow } from './useUiConfigFollow';
import { useWindowNotices } from './useWindowNotices';
import { useWindowFocus } from './useWindowFocus';

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
  // Every open session, which the sessions sidebar lists. It shows
  // while two or more are open, until the sessions toggle hides it for
  // this window or the window grows too narrow to hold it, where the
  // toggle slides it over the terminal instead.
  const sessions = useSessions();
  const panelOpen = panelLayout?.panel_open ?? true;
  // The status line carries your vitals with the panel hidden, or with
  // Show your vitals in on Status line.
  const vitalsPlace = useVitalsOptions().place;
  const lineShowsVitals = !panelOpen || vitalsPlace === 'status';
  // Read your prompt shows in the palette while the reader is on.
  const readerOn = useScreenReader().screen_reader;
  // The sidebar hands the caret back to the command line as it goes.
  const sessionsSidebar = useSessionsSidebar(sessions.length, panelOpen, () =>
    inputRef.current?.focus(),
  );
  const sessionsShown = sessionsSidebar.shown;
  // Rename session… names the selected session in its row while the
  // sidebar shows, and in the session popover's own form while it
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
  // session keys ask after, since a macro on a session key keeps it.
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
  // The prompt card and the writing card over your prompt, and what
  // opens each.
  const {
    promptCard,
    cardBinding,
    writingCard,
    openPromptCard,
    openWriting,
    openWritingFromEditor,
    charStatus,
    writeKinds,
    cardBand,
    setCardBand,
    promptCardHost,
    closePromptCard,
    closeWriting,
  } = useCardRequests({
    termRef,
    terminalAreaRef,
    focusInput,
    shownPanes,
    panelLoaded: panelLayout !== null,
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

  // Click to type: a click in the window, the window coming to the
  // front and a copy put the caret back on the command line.
  const { handleTerminalMouseUp, handleAppMouseUp } = useWindowFocus({ focusInput, middleClick });

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

  // Everything the palette can reach, rebuilt fresh at each open so
  // labels track live state.
  const paletteDeps = () =>
    paletteDepsFor({
      connected,
      status,
      connection,
      panelOpen,
      splitOpen,
      toggleSplit,
      shownPanes,
      openFind,
      renameSession,
      closing,
      sessions,
      selected,
      sessionsSidebar,
      inputRef,
      promptShow,
      openPromptCard,
      writeKinds,
      charStatus,
      openWriting,
      readerOn,
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
    scrollbackLines,
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

  // The tones, toasts and notices the window raises on its own.
  useWindowNotices(writeLive);

  const inputElement = (
    <Input
      ref={inputRef}
      enabled={connected}
      macroKeys={macroKeys}
      fontKey={`${fontFamily}|${fontSize}`}
      onError={handleError}
      onOpenWriting={openWritingFromEditor}
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

  const terminalAreaElement = terminalArea({
    terminalAreaRef,
    splitOpen,
    handleTerminalMouseUp,
    setTerminalMenu,
    finds,
    opened,
    selected,
    findToolbarRef,
    submitFind,
    closeFind,
    findOpen,
    historyScrollPos,
    historyReady,
    termRef,
    renderFamily,
    fontSize,
    terminalLineHeight,
    themeTerminalColors,
    blinkText,
    scrollbackLines,
    historyTermRef,
    onHistoryLoaded,
    onHistoryScroll,
    onTerminalReady,
    onScrollbackLoaded,
    onFindResults,
    setCellSize,
    promptLifted,
    dockLent,
    dockShows,
    promptPinned,
    promptShow,
    cellSize,
    brightBold,
  });

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
      reader={<ScreenReaderFeed />}
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
