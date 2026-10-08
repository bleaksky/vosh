// The main window's commands. The shortcut keys and the macOS menu bar
// run each one by its palette id through one dispatcher, #help from the
// command line opens Help, and the menu bar mirrors this window.

import { useEffect, useRef, useState, type RefObject } from 'react';
import { useTauriEvent } from '../ipc/useTauriEvent';
import { menuCopy, openHelpWindow, openSettingsWindow, subscribeHelpOpen } from '../ipc/windows';
import {
  buildMenuState,
  commandRepeats,
  listenAppMenu,
  pageHasSelection,
  requestSessionMenu,
  resolveShortcut,
  setAppMenuState,
} from '../lib/appMenu';
import { canonicalKeyFromEvent } from '../automation/macroKeys';
import type { MacroKeys } from '../input/useMacroKeys';
import { helpNoMatchNotice, helpOpensOn, openHelpTopic } from '../lib/helpLink';
import { isMacPlatform, shortcutKey } from '../lib/shortcuts';
import { getImmState, subscribeImmState } from '../stores/gmcp/immStore';
import { getSnoops, useSnoops } from '../stores/session/snoopStore';
import { goTo, sessionAt, sessionStep } from '../stores/session/sessionsStore';
import type { Connection } from '../stores/session/useConnection';
import {
  getNativeScroll,
  startNativeScroll,
  subscribeNativeScroll,
} from '../terminal/native/nativeScroll';
import type { TerminalHandle } from '../terminal/terminalHandle';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { getCurrentThemeId } from '../theme/theme';
import {
  buildPaletteEntries,
  themeEntries,
  themesInGalleryOrder,
  type PaletteDeps,
} from './overlays/palette';
import { openNewSession } from './newSession';
import { goToSnoop, requestSnoop, snoopHasCaret } from './snoopKeys';
import type { ScrollbackFind } from './useFind';
import type { ScrollbackSplit } from './useScrollbackSplit';

interface CommandInputs
  extends
    Pick<ScrollbackSplit, 'splitOpen' | 'toggleSplit'>,
    Pick<ScrollbackFind, 'findOpen' | 'openFind' | 'findToolbarRef'> {
  /** The session, for Connect and the menu bar. */
  connection: Connection;
  /** The selected session's macros. One on a session key keeps the key. */
  macroKeys: MacroKeys;
  /** Close the selected session, asking first while it is connected. */
  closeSession: () => void;
  /** Close the window, asking first while a session is connected. */
  closeWindow: () => void;
  /** Quit Vosh, asking first while two or more sessions are connected.
   *  The macOS menu bar sends it only then. */
  quit: () => void;
  paletteOpen: boolean;
  setPaletteOpen: (open: boolean) => void;
  /** Shows or hides the panel, and puts the caret back on the command
   *  line when it sat in the panel. */
  togglePanel: () => void;
  /** Puts the caret on the command line. */
  focusInput: () => void;
  panelOpen: boolean;
  /** The paneKey of every pane the panel tree holds. */
  shownPanes: readonly string[];
  /** How many sessions are open, and whether you keep the sidebar
   *  showing them, though a narrow window can fold it. */
  sessionCount: number;
  sessionsShown: boolean;
  /** Hide the sessions sidebar in this window, or show it again. */
  toggleSessions: () => void;
  /** The live pane and the history pane, whose selection Copy takes. */
  termRef: RefObject<TerminalHandle>;
  historyTermRef: RefObject<TerminalHandle>;
  /** Writes a notice to the terminal. */
  writeLive: (text: string) => void;
  /** Everything the palette reaches, for an id the dispatcher hands to
   *  its palette entry. */
  paletteDeps: () => PaletteDeps;
}

interface AppCommands {
  /** Run a command by its palette id. */
  runCommand: (id: string, opts?: { repeat?: boolean }) => void;
  /** Another window changed the custom themes, which the menu bar lists. */
  themesChanged: () => void;
}

export function useAppCommands({
  connection,
  macroKeys,
  closeSession,
  closeWindow,
  quit,
  splitOpen,
  toggleSplit,
  findOpen,
  openFind,
  findToolbarRef,
  paletteOpen,
  setPaletteOpen,
  togglePanel,
  focusInput,
  panelOpen,
  shownPanes,
  sessionCount,
  sessionsShown,
  toggleSessions,
  termRef,
  historyTermRef,
  writeLive,
  paletteDeps,
}: CommandInputs): AppCommands {
  // What the macOS menu bar mirrors beyond the panel and the session: a
  // tick for every theme apply or custom theme change, whether the MUD
  // offers staff queues, whether the native grid is scrolled back, and
  // how many snoop tabs the session has.
  const [themeTick, setThemeTick] = useState(0);
  const [staffOffered, setStaffOffered] = useState(false);
  const [nativeScrolled, setNativeScrolled] = useState(false);
  // The selected session's snoop tabs, for Go to snoop in View.
  const snoops = useSnoops().tabs.length;

  // Window shortcuts, in the capture phase so they fire before xterm's
  // own keybindings, the webview's find and reload, and the command
  // line's macros. macOS binds Cmd, because Ctrl belongs to your macros
  // there, and Ctrl with Cmd only for the sessions toggle. Windows and
  // Linux bind Ctrl. The keys live in lib/appShortcuts.json, which the
  // macOS menu bar reads too.
  //   Mod+K        command palette (toggles)
  //   Mod+F        find in scrollback (again refocuses the find field)
  //   Mod+R        connect the selected session. Ctrl+R never reloads
  //                the page on Windows, even while connected.
  //   Mod+,        settings
  //   Mod+/        help
  //   Mod+Shift+L  show or hide the panel
  //   Ctrl+Cmd+S   show or hide the sessions sidebar, Ctrl+Shift+S on
  //                Windows and Linux
  //   Mod+\        open or close the scrollback split
  //   Mod+J        into the snoop, and in a snoop to the next tab. With
  //                no snoop open the key stays the page's.
  //   Mod+T        new session
  //   Mod+W        close the session, or the window with one session
  //   Mod+Shift+W  close the window
  //   Mod+Shift+]  the next session, and Mod+Shift+[ the previous one
  //   Mod+1 to 9   the session at that place in the list
  //   Cmd+Option+1 to 4 on macOS and Ctrl+Shift+1 to 4 elsewhere
  //                Timers, Aliases, Triggers and Macros in Settings, by
  //                the physical digit key, since Option or Shift with 1
  //                types another character
  // A key this handler takes never reaches the menu bar, and the menu
  // bar sends its commands through runCommand below too, so each press
  // runs once. Keys match through shortcutKey, so a Cyrillic or Greek
  // layout still reaches them by the physical key. A macro the selected
  // session's profile binds to one of the session keys or Settings keys
  // keeps the key (Sessions Q11), as does one on the sessions toggle's
  // key (Sessions toggle T3): nothing here or in the menu bar takes
  // it, and the command line fires the macro.
  const shortcutState = useRef({ findOpen, paletteOpen, live: connection.live });
  const runCommandRef = useRef<(id: string, opts?: { repeat?: boolean }) => void>(() => {});
  useEffect(() => {
    const mac = isMacPlatform();
    const onKey = (e: globalThis.KeyboardEvent) => {
      // Ctrl beside Cmd on macOS reaches only a key whose spec names
      // Ctrl, the sessions toggle's.
      const primary = mac ? e.metaKey : e.ctrlKey && !e.metaKey;
      if (!primary) return;
      const press = {
        key: shortcutKey(e),
        code: e.code,
        shift: e.shiftKey,
        ctrl: mac && e.ctrlKey,
        alt: e.altKey,
      };
      const hit = resolveShortcut(
        press,
        () => macroKeys.bound(canonicalKeyFromEvent(e)),
        () => getSnoops().tabs.length > 0,
      );
      if (!hit) return;
      e.preventDefault();
      if (hit.kind === 'macro') return;
      e.stopPropagation();
      if (hit.kind === 'run') runCommandRef.current(hit.id, { repeat: e.repeat });
      else if (hit.kind === 'goto' && !e.repeat) goTo(sessionAt(hit.place));
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [macroKeys]);

  // `#help <words>` from the command line (src-tauri input::help_query).
  // A topic id or number opens that topic. Other words open Help on its
  // search when a topic matches, and the terminal says so when none does.
  useTauriEvent(subscribeHelpOpen, (payload) => {
    const words = typeof payload === 'string' ? payload.trim() : '';
    if (!helpOpensOn(words)) {
      if (words.length > 0) {
        writeLive(`\x1b[38;5;244m${helpNoMatchNotice(words)}\x1b[0m\r\n`);
      }
      return;
    }
    openHelpTopic(words);
  });

  // Menu bar commands (macOS only). Each arrives with its palette id.
  useTauriEvent(
    (cb) => (isMacPlatform() ? listenAppMenu(cb) : Promise.resolve(() => {})),
    (id: string) => runCommandRef.current(id),
  );

  // Edit, then Copy, in the menu bar. With no selection of the page's
  // own, the terminal selection wins, the same as Cmd+C in the command
  // line and Copy in the terminal menu. Otherwise the system copies the
  // field or page selection.
  const copyFromMenu = () => {
    if (snoopHasCaret()) {
      requestSnoop('copy');
      return;
    }
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
        togglePanel();
        return;
      case 'palette':
        setPaletteOpen(!inPalette);
        if (inPalette) focusInput();
        return;
      case 'find':
        // Cmd F in a snoop finds in that snoop.
        if (snoopHasCaret()) {
          requestSnoop('find');
          return;
        }
        // Open just the toolbar. Whether to open the split is decided
        // per search: only when a match would scroll the live pane up
        // off its tail (see submitFind in useFind.ts).
        if (finding) findToolbarRef.current?.focus();
        else openFind();
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
      case 'snoop':
        goToSnoop();
        return;
      case 'session-edit':
        requestSessionMenu({ mode: 'edit' });
        return;
      case 'session-new':
        void openNewSession();
        return;
      case 'session-next':
        goTo(sessionStep(1));
        return;
      case 'session-previous':
        goTo(sessionStep(-1));
        return;
      case 'session-close':
        closeSession();
        return;
      case 'sessions-sidebar':
        toggleSessions();
        return;
      case 'close-window':
        closeWindow();
        return;
      case 'quit':
        quit();
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
        redialing: connection.redialing,
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
        sessions: sessionCount,
        sessionsShown,
        snoops,
      }),
    );
  }, [
    connection.live,
    connection.redialing,
    connection.world,
    panelOpen,
    splitOpen,
    nativeScrolled,
    shownPanes,
    staffOffered,
    themeTick,
    sessionCount,
    sessionsShown,
    snoops,
  ]);

  return {
    runCommand,
    themesChanged: () => setThemeTick((n) => n + 1),
  };
}
