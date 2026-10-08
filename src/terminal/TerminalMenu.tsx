import {
  Fragment,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from 'react';
import { type TerminalHandle } from './terminalHandle';
import { nativeSurfaceEnabled } from './terminalRenderer';
import { type InputHandle } from '../input/Input';
import { submenuAt } from '../ui/menuPlacement';
import { pointAt, pointerLeft } from '../ui/menuAim';
import { MenuItem, MenuSeparator, MenuSurface } from '../ui/MenuSurface';
import { ChevronRightIcon } from '../ui/icons';
import { nativeSurfaceCopy, nativeSurfaceSelectAll } from '../ipc/nativeSurface';
import { scrollbackClear } from '../ipc/terminal';
import { openHelpWindow } from '../ipc/windows';
import { openPaneSubmenu, type PaneSubmenuState } from '../panel/affects/affectsDisplay';
import APP_SHORTCUTS from '../lib/appShortcuts.json';
import { shortcutLabel } from '../lib/shortcuts';
import { openSettingsTab } from '../lib/settingsLink';
import { getUiConfig } from '../ipc/uiConfig';
import { logsWorld } from '../settings/general/scene';
import { getSelected, getSessions } from '../stores/session/sessionsStore';
import { loadTarget, targetOf } from '../stores/session/useConnection';
import { SETTINGS_MENU, type SettingsMenuRow } from './settingsMenu';
import type { WritingKind } from '../ipc/writing';
import { KINDS } from '../writing/kinds';

interface Props {
  /** Pointer position in viewport coordinates. The menu opens with its
   *  top-left corner here, clamped so it never overflows the window. */
  x: number;
  y: number;
  /** The selected session, whose terminal the menu acts on. */
  session: number;
  termRef: RefObject<TerminalHandle | null>;
  inputRef: RefObject<InputHandle | null>;
  onOpenFind: () => void;
  /** Open the prompt card over your prompt. */
  onCustomizePrompt: () => void;
  /** The boards you can write on, which Write lists over your
   *  description and history. */
  writeKinds: WritingKind[];
  /** Open the writing card on a kind. */
  onWrite: (kind: WritingKind) => void;
  onClose: () => void;
}

interface Item {
  id: string;
  label: string;
  /** Shortcut spec for the trailing hint, like 'Mod+C'. */
  keys?: string;
  danger?: boolean;
  /** The row shows but does nothing, like Save a scene… while the
   *  profile logs nothing. */
  disabled?: boolean;
  /** The row opens a list beside the menu instead of running. */
  submenu?: Sub;
  run: () => void;
}

/** The lists a row opens beside the menu. */
type Sub = 'write' | 'settings';

/** The id of the list each row controls. */
const LIST_ID: Record<Sub, string> = {
  write: 'terminal-menu-write',
  settings: 'terminal-menu-settings',
};

/** How close the menu may sit to the window edge after clamping. */
const EDGE_MARGIN = 8;

// Right-click menu for the terminal: a 232 wide floating surface with 6
// of inner padding, 30 tall rows, shortcut hints on the right in the
// platform's glyphs, and hairline separators. Customize prompt… comes
// first on every row, since Vosh may not know yet which row is your
// prompt, and opens the prompt card over it. Write under it opens the
// kinds of text the writing card takes. Settings opens a list beside
// the menu, built like the pane menus' submenus, that goes straight to
// Triggers, Aliases, Macros and Timers, to each Settings page, and to
// Help. Clear scrollback is the one destructive item and comes last.
// Every action routes through the same path the keyboard uses (the
// native copy command, the input row's insert, the find bar, the
// palette's Settings links), so the menu never grows a second
// implementation. It takes focus while open so the arrow keys and Enter
// drive it, then hands focus back.
export function TerminalMenu({
  x,
  y,
  session,
  termRef,
  inputRef,
  onOpenFind,
  onCustomizePrompt,
  writeKinds,
  onWrite,
  onClose,
}: Props) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const subRowRefs = useRef<Partial<Record<Sub, HTMLButtonElement | null>>>({});
  const [pos, setPos] = useState({ x, y });
  const [active, setActive] = useState(-1);
  // The list open beside the menu, and whether it opened from the
  // keyboard and so takes focus.
  const [sub, setSub] = useState<PaneSubmenuState<Sub> | null>(null);
  // Save a scene… shows disabled while the profile logs nothing on the
  // world the selected session dials, which the menu reads as it opens.
  const [logged, setLogged] = useState(true);
  useEffect(() => {
    let cancelled = false;
    const row = getSessions().find((r) => r.id === getSelected()) ?? null;
    const { host } = targetOf(row, loadTarget());
    getUiConfig()
      .then((config) => {
        if (!cancelled) setLogged(logsWorld(config.log_sessions, host));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // Clamp to the viewport once the menu has a measurable size. Re-runs
  // when a second right-click moves the anchor while the menu is open.
  useLayoutEffect(() => {
    const el = menuRef.current;
    if (!el) return;
    const maxX = window.innerWidth - el.offsetWidth - EDGE_MARGIN;
    const maxY = window.innerHeight - el.offsetHeight - EDGE_MARGIN;
    setPos({
      x: Math.max(EDGE_MARGIN, Math.min(x, maxX)),
      y: Math.max(EDGE_MARGIN, Math.min(y, maxY)),
    });
  }, [x, y]);

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    menuRef.current?.focus({ preventScroll: true });
    return () => {
      const current = document.activeElement;
      if (!current || current === document.body) previous?.focus();
    };
  }, []);

  // Outside press and Escape close. Pointerdown so the close fires
  // before any click handler inside an unrelated element. The Settings
  // list draws apart from the menu, so a press on it counts as inside.
  // Escape also closes when focus has drifted out of the menu.
  useEffect(() => {
    const onPointer = (e: PointerEvent) => {
      if (!menuRef.current) return;
      if (e.target instanceof Node && menuRef.current.contains(e.target)) return;
      if (e.target instanceof Element && e.target.closest('[data-menu-surface]')) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('pointerdown', onPointer);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onPointer);
      document.removeEventListener('keydown', onKey);
    };
  }, [onClose]);

  const runCopy = () => {
    // Same fork as the Cmd+C path in Input.tsx: the native surface owns
    // the visible selection when enabled, xterm otherwise.
    if (nativeSurfaceEnabled()) {
      void nativeSurfaceCopy(session).catch(() => {});
      return;
    }
    const text = termRef.current?.getSelection() ?? '';
    if (text.length > 0) {
      void navigator.clipboard.writeText(text).catch(() => {});
    }
  };

  // The same fork as Cmd+A on an empty command line (see Input): the
  // native grid selects through its own command, xterm otherwise.
  const runSelectAll = () => {
    if (nativeSurfaceEnabled()) {
      void nativeSurfaceSelectAll(session).catch(() => {});
      return;
    }
    termRef.current?.selectAll();
  };

  const runPaste = () => {
    void navigator.clipboard
      .readText()
      .then((text) => {
        if (text.length > 0) inputRef.current?.insert(text);
      })
      .catch(() => {});
  };

  // Clear scrollback empties what you can scroll back through on either
  // renderer, and the lines Vosh keeps for the next launch. xterm clears
  // its own buffer, and the backend clears the native grid.
  const runClear = () => {
    if (!nativeSurfaceEnabled()) termRef.current?.clear();
    void scrollbackClear(session).catch(() => {});
  };

  // Groups split by separators.
  const groups: Item[][] = [
    [
      { id: 'customize-prompt', label: 'Customize prompt…', run: onCustomizePrompt },
      { id: 'write', label: 'Write', submenu: 'write', run: () => openSub('write', true) },
    ],
    [
      { id: 'copy', label: 'Copy', keys: APP_SHORTCUTS.copy, run: runCopy },
      { id: 'paste', label: 'Paste', keys: 'Mod+V', run: runPaste },
      { id: 'select-all', label: 'Select all', keys: 'Mod+A', run: runSelectAll },
    ],
    [
      { id: 'find', label: 'Find in scrollback…', keys: APP_SHORTCUTS.find, run: onOpenFind },
      {
        id: 'scene',
        label: 'Save a scene…',
        disabled: !logged,
        run: () => openSettingsTab('general:scene'),
      },
    ],
    [
      {
        id: 'settings',
        label: 'Settings',
        submenu: 'settings',
        run: () => openSub('settings', true),
      },
    ],
    [{ id: 'clear', label: 'Clear scrollback', danger: true, run: runClear }],
  ];
  const items = groups.flat();
  const subAt = (which: Sub) => items.findIndex((item) => item.submenu === which);

  // Open a list beside its row. From the keyboard or a click it takes
  // focus on its first row. Pointing at the row leaves focus in the
  // menu, and leaves a list the keyboard opened as it is.
  function openSub(which: Sub, focus: boolean) {
    setActive(subAt(which));
    setSub((prev) => openPaneSubmenu(prev, which, focus));
  }

  // Close the list and hand focus back to the menu, so the arrow keys
  // keep working when focus was in the list.
  const closeSub = () => {
    setSub(null);
    const el = menuRef.current;
    if (el && document.activeElement !== el) el.focus({ preventScroll: true });
  };

  // Highlight a row. A row that opens no list closes the list.
  const highlight = (i: number) => {
    setActive(i);
    if (sub && !items[i]?.submenu) closeSub();
  };

  // Every item closes the menu first, then runs, the same order the
  // palette uses, so an action that moves focus wins over the menu.
  // Settings opens its list instead.
  const pick = (item: Item | undefined) => {
    if (!item || item.disabled) return;
    if (!item.submenu) onClose();
    item.run();
  };

  const pickRow = (row: SettingsMenuRow) => {
    onClose();
    if (row.link === null) openHelpWindow();
    else openSettingsTab(row.link);
  };

  const pickKind = (kind: WritingKind) => {
    onClose();
    onWrite(kind);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      highlight((active + 1) % items.length);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      highlight(active <= 0 ? items.length - 1 : active - 1);
    } else if (e.key === 'Home') {
      e.preventDefault();
      highlight(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      highlight(items.length - 1);
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      const which = items[active]?.submenu;
      if (which) openSub(which, true);
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      if (sub) closeSub();
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      pick(items[active]);
    } else if (e.key === 'Escape') {
      // One level at a time: the list, then the menu.
      e.preventDefault();
      e.stopPropagation();
      if (sub) closeSub();
      else onClose();
    } else if (e.key === 'Tab') {
      e.preventDefault();
    }
  };

  // The list beside its row, built like the pane menus' submenus. It
  // opens right of the menu level with its row, flips left at the right
  // edge and up at the bottom edge, and Esc or ArrowLeft steps back to
  // the row. It draws apart from the menu, so its keys never reach the
  // menu's own.
  let list: ReactNode = null;
  const row = sub ? subRowRefs.current[sub.which] : null;
  if (sub && row && menuRef.current) {
    const at = submenuAt(row.getBoundingClientRect(), menuRef.current.getBoundingClientRect());
    const back = () => {
      setActive(subAt(sub.which));
      closeSub();
    };
    list =
      sub.which === 'write' ? (
        <MenuSurface
          id={LIST_ID.write}
          label="Write"
          nested
          autoFocus={sub.focus}
          className="pane-menu-sub"
          at={at}
          onClose={back}
        >
          {writeKinds.map((kind) => (
            <MenuItem key={kind} onSelect={() => pickKind(kind)}>
              {KINDS[kind].menu}
            </MenuItem>
          ))}
          <MenuSeparator />
          <MenuItem onSelect={() => pickKind('description')}>{KINDS.description.menu}</MenuItem>
          <MenuItem onSelect={() => pickKind('history')}>{KINDS.history.menu}</MenuItem>
        </MenuSurface>
      ) : (
        <MenuSurface
          id={LIST_ID.settings}
          label="Settings"
          nested
          autoFocus={sub.focus}
          className="pane-menu-sub"
          at={at}
          onClose={back}
        >
          {SETTINGS_MENU.map((group, g) => (
            <Fragment key={group[0].id}>
              {g > 0 && <MenuSeparator />}
              {group.map((entry) => (
                <MenuItem
                  key={entry.id}
                  onSelect={() => pickRow(entry)}
                  trailing={
                    entry.keys ? (
                      <kbd className="ov-menu-keys">{shortcutLabel(entry.keys)}</kbd>
                    ) : null
                  }
                >
                  {entry.label}
                </MenuItem>
              ))}
            </Fragment>
          ))}
        </MenuSurface>
      );
  }

  let index = -1;
  return (
    <>
      <div
        ref={menuRef}
        className="ov-menu ov-terminal-menu"
        role="menu"
        aria-label="Terminal"
        tabIndex={-1}
        style={{ left: pos.x, top: pos.y }}
        onKeyDown={onKeyDown}
        // A row stays the active one while its list is open, so the
        // pointer can cross into the list, or leave both, and Enter,
        // the arrows and the highlight still agree on where you are.
        onPointerLeave={() => {
          if (!sub) setActive(-1);
        }}
        // Keep the window's click-to-type handler from pulling focus to
        // the command line when you press the menu's padding.
        onMouseUp={(e) => e.stopPropagation()}
      >
        {groups.map((group, g) => (
          <div key={group[0].id} role="none" className="ov-menu-group">
            {g > 0 && <div role="separator" className="ov-menu-sep" />}
            {group.map((item) => {
              index += 1;
              const i = index;
              // The active row is the one lit. While a list is open, that
              // row is the one that opened it.
              const lit = i === active;
              return (
                <button
                  key={item.id}
                  ref={(el) => {
                    if (item.submenu) subRowRefs.current[item.submenu] = el;
                  }}
                  type="button"
                  role="menuitem"
                  tabIndex={-1}
                  aria-disabled={item.disabled || undefined}
                  aria-haspopup={item.submenu ? 'menu' : undefined}
                  aria-expanded={item.submenu ? sub?.which === item.submenu : undefined}
                  aria-controls={
                    item.submenu && sub?.which === item.submenu ? LIST_ID[item.submenu] : undefined
                  }
                  className={`ov-menu-item${lit ? ' is-active' : ''}${
                    item.danger ? ' is-danger' : ''
                  }`}
                  // A row the pointer crosses on its way into the open
                  // list waits until it turns away or rests (menuAim.ts).
                  onPointerMove={(e) => {
                    const which = item.submenu;
                    pointAt(e.currentTarget, () => {
                      if (which) openSub(which, false);
                      else if (i !== active || sub) highlight(i);
                    });
                  }}
                  onPointerLeave={(e) => pointerLeft(e.currentTarget)}
                  // A row that takes focus, as Show me's ring gives it,
                  // is the row the keys act on.
                  onFocus={() => setActive(i)}
                  onClick={() => pick(item)}
                >
                  <span className="ov-menu-label">{item.label}</span>
                  {item.keys && <kbd className="ov-menu-keys">{shortcutLabel(item.keys)}</kbd>}
                  {item.submenu && <ChevronRightIcon className="pane-menu-chevron" />}
                </button>
              );
            })}
          </div>
        ))}
      </div>
      {list}
    </>
  );
}
