import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Terminal } from '../terminal/Terminal';
import type { TerminalHandle } from '../terminal/terminalHandle';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { Input, type InputHandle } from '../input/Input';
import { Resizable } from '../terminal/Resizable';
import { UpdateNotice } from './overlays/UpdateNotice';
import { Toasts } from './overlays/Toasts';
import { FindToolbar } from '../terminal/FindToolbar';
import { TerminalMenu } from '../terminal/TerminalMenu';
import { ScrollDepth } from '../terminal/ScrollDepth';
import { AppShell } from './AppShell';
import { TitleBand } from './TitleBand';
import { StatusLine } from './StatusLine';
import { PanelHost } from '../panel/PanelHost';
import {
  panelWidthOf,
  setPanelWidth,
  togglePanelOpen,
  usePanelLayout,
} from '../panel/panelLayoutStore';
import { addPaneType, togglePane } from '../panel/paneActions';
import {
  promptConfigGet,
  promptConfigSet,
  promptCodeReaderSet,
  subscribePromptCardOpen,
} from '../ipc/prompt';
import { promptPreviewSet } from '../ipc/promptDesign';
import { disconnectSession, setWindowSize, onState, type StatePayload } from '../ipc/session';
import { terminalLocalWrite } from '../ipc/terminal';
import { TERMINAL_LINE_HEIGHTS } from '../ipc/uiConfig';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { openHelpWindow, openSettingsWindow } from '../ipc/windows';
import { subscribeMigrationApplied } from '../ipc/wizard';
import { listenForQuitFlush } from '../lib/pendingWrites';
import { startStores } from '../stores';
import { pushToast } from '../stores/toasts';
import { showLaunchNotices, showMigrationApplied } from './launchNotices';
import { startGamePromptToasts } from '../prompt/gamePromptToast';
import { CommandPalette } from './overlays/CommandPalette';
import type { PaletteDeps } from './overlays/palette';
import { ConfirmDialog } from '../ui/ConfirmDialog';
import { openSettingsTab } from '../lib/settingsLink';
import { getNativeScroll } from '../terminal/native/nativeScroll';
import { allPanes, PANE_TYPES } from '../panel/paneLayout';
import { offeredPaneTypes } from '../panel/paneTypes';
import { noteConnectionError } from '../stores/session/connectionStore';
import { getSelected, useSelected } from '../stores/session/sessionsStore';
import { useConnection } from '../stores/session/useConnection';
import { useEscape } from '../lib/escapeStack';
import { usePromptShow } from '../prompt/showState';
import { PromptDock } from '../prompt/PromptDock';
import { PromptCard, type PromptCardHost } from '../prompt/PromptCard';
import { nextCardRequest, type CardRequest, type CardRequestView } from '../prompt/cardRules';
import { usePinnedDockRows } from '../stores/session/pinnedPromptStore';
import { lentRows, type CellSize } from '../prompt/pinnedDock';
import { useAppCommands } from './useAppCommands';
import { useFind } from './useFind';
import { useNativeSurfaceBridge } from './useNativeSurfaceBridge';
import { useScrollbackSplit } from './useScrollbackSplit';
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
  const panelOpen = panelLayout?.panel_open ?? true;
  const panelWidth = panelWidthOf(panelLayout);
  const shownPanes = useMemo(() => (panelLayout ? allPanes(panelLayout.root) : []), [panelLayout]);
  const termRef = useRef<TerminalHandle | null>(null);
  const historyTermRef = useRef<TerminalHandle | null>(null);
  const inputRef = useRef<InputHandle | null>(null);
  // Puts the caret back on the command line.
  const focusInput = () => inputRef.current?.focus();
  // Write text the page draws itself (your typed echo, error notices) to
  // xterm, and through terminal_local_write to the native grid and the
  // session on either renderer. The session closes the open row, since
  // the text now follows it, so it never repaints over your echo. Your
  // line goes out by its own call, so the session can hear of the echo
  // after the reply. It closes only the rows that came before the newest
  // output the renderer that shows took: xterm names it here, and the
  // native grid names its own as it takes the text.
  const writeLive = (text: string) => {
    const term = termRef.current;
    term?.write(text);
    const after = nativeSurfaceEnabled() ? null : (term?.outputTaken() ?? 0);
    void terminalLocalWrite(text, after, getSelected()).catch(() => {});
  };
  // Write text into the terminal of `session`. The selected session's
  // shows it now. A session behind takes it in its own grid through
  // terminal_local_write, which closes its open row too, and shows it on
  // its selection.
  const writeTo = (session: number, text: string) => {
    if (session === getSelected()) writeLive(text);
    else void terminalLocalWrite(text, null, session).catch(() => {});
  };
  // An action failed, a connect, a send or a disconnect. The session it
  // was for shows the error in its title band and its terminal.
  const handleError = (message: string, session = getSelected()) => {
    noteConnectionError(session, message);
    writeTo(session, `\r\n\x1b[31m[${message}]\x1b[0m\r\n`);
  };
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
  // Every request counts, so the open card hears a repeat of one.
  const openPromptCard = useCallback(
    (view: CardRequestView) => setPromptCard((prev) => nextCardRequest(prev, view)),
    [],
  );
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
    focusInput();
  };

  // Customize… in Settings, and anything else in another window, opens
  // the card here and brings this window forward.
  useTauriEvent(subscribePromptCardOpen, (request) => {
    openPromptCard(request.view ?? 'design');
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
    termRef,
    historyTermRef,
    terminalAreaRef,
    focusInput,
  });

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
  const { findOpen, openFind, findToolbarRef, findResults, closeFind, submitFind, onFindResults } =
    useFind({
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
    openFind,
    openSettings: openSettingsWindow,
    openSettingsTab,
    connect: () => void connection.connect(),
    disconnect: () => void disconnectSession(getSelected()),
    insertInput: (text) => inputRef.current?.insert(text),
    promptShow: promptShow?.capture ? promptShow.show : null,
    openPromptCard: (view) => openPromptCard(view === 'text' ? 'text' : 'design'),
    promptDraw: promptShow?.capture ? promptShow.draw : null,
    setPromptDraw: (on) => {
      const session = getSelected();
      void promptConfigGet(session)
        .then((config) => promptConfigSet({ ...config, draw: on }, { session }))
        .catch((e: unknown) => pushToast({ kind: 'error', message: String(e) }));
    },
  });

  // The window shortcuts, the macOS menu bar and #help.
  const { runCommand, confirmClose, setConfirmClose, closeMainWindow, themesChanged } =
    useAppCommands({
      connection,
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

  // Keep the native surface under the page in step with this window.
  useNativeSurfaceBridge({
    blinkText,
    promptLifted,
    cardBand,
    focusInput,
  });

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

  // The connection store keeps where each session stands. Here the
  // window says so: the toasts speak for the selected session only, and
  // a session behind shows its drop on its row.
  useTauriEvent(onState, (payload: StatePayload) => {
    const shown = payload.session === getSelected();
    if (payload.kind === 'disconnected') {
      // A reason means the link dropped out from under us; a clean
      // user-initiated disconnect carries none and stays quiet. The
      // reason goes into the terminal of the session that dropped.
      if (payload.reason) {
        if (termRef.current) {
          writeTo(payload.session, `\r\n\x1b[31m[${payload.reason}]\x1b[0m\r\n`);
        }
        if (shown) pushToast({ kind: 'error', message: 'Connection lost', meta: payload.reason });
      }
    } else if (payload.kind === 'connected') {
      if (shown) {
        pushToast({
          kind: 'success',
          message: 'Connected',
          meta: `${payload.host}:${payload.port}`,
        });
      }
      // Push the current terminal size on every (re)connect so the
      // negotiator advertises the live cols × rows via NAWS as soon
      // as the server asks. MUDs that honor NAWS wrap at this width
      // server-side, which is the right answer to word wrap. Every
      // session plays in this one window, so a session behind takes
      // the same size.
      const handle = termRef.current;
      if (handle) {
        const { cols, rows } = handle.windowSize();
        void setWindowSize(cols, rows, payload.session).catch(() => {});
      }
    }
  });

  const inputElement = (
    <Input
      ref={inputRef}
      enabled={connected}
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
        <div className="terminal-pane terminal-pane-live">
          <Terminal
            fontFamily={renderFamily}
            fontSize={fontSize}
            lineHeight={TERMINAL_LINE_HEIGHTS[terminalLineHeight]}
            themeTerminalColors={themeTerminalColors}
            blinkText={blinkText}
            onReady={(handle) => {
              termRef.current = handle;
            }}
            // After the restored scrollback, so what launch has to tell
            // you lands below it instead of scrolling away above.
            onScrollbackLoaded={() => void showLaunchNotices(writeLive)}
            onResultsChanged={onFindResults}
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
          blinkText={blinkText}
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
          onTogglePanel={() => togglePanelKeepingCaret(focusInput)}
          onTogglePalette={() => (paletteOpen ? closePalette() : setPaletteOpen(true))}
          onOpenSettings={() => runCommand('settings')}
          paneTree={panelLayout?.root ?? null}
          onAddPane={addPaneType}
          onMenuClosed={focusInput}
        />
      }
      terminal={terminalAreaElement}
      input={inputElement}
      statusLine={<StatusLine connected={connection.live} showVitals={!panelOpen} />}
      panel={<PanelHost promptShow={promptShow} textSize={panelTextPx} />}
    >
      <UpdateNotice />
      <Toasts />
      {terminalMenu && (
        <TerminalMenu
          x={terminalMenu.x}
          y={terminalMenu.y}
          termRef={termRef}
          inputRef={inputRef}
          onOpenFind={openFind}
          onCustomizePrompt={() => openPromptCard('design')}
          onClose={() => setTerminalMenu(null)}
        />
      )}
      {promptCard && (
        // A selection mounts the card again for the session it brings to
        // the front, and the card it leaves puts that session's live
        // prompt back as it goes.
        <PromptCard
          key={selected}
          session={selected}
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

export default MainWindow;
