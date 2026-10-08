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
import { MenuItem, MenuSeparator, MenuSurface } from '../ui/MenuSurface';
import { ChevronRightIcon } from '../ui/icons';
import { nativeSurfaceCopy, nativeSurfaceSelectAll } from '../ipc/nativeSurface';
import { scrollbackClear } from '../ipc/terminal';
import { openHelpWindow } from '../ipc/windows';
import { openPaneSubmenu, type PaneSubmenuState } from '../panel/affects/affectsDisplay';
import APP_SHORTCUTS from '../lib/appShortcuts.json';
import { shortcutLabel } from '../lib/shortcuts';
import { openSettingsTab } from '../lib/settingsLink';
import { SETTINGS_MENU, type SettingsMenuRow } from './settingsMenu';

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
  onClose: () => void;
}

interface Item {
  id: string;
  label: string;
  /** Shortcut spec for the trailing hint, like 'Mod+C'. */
  keys?: string;
  danger?: boolean;
  /** The row opens the Settings list instead of running. */
  submenu?: boolean;
  run: () => void;
}

/** The id of the Settings list, which the Settings row controls. */
const SETTINGS_LIST_ID = 'terminal-menu-settings';

/** How close the menu may sit to the window edge after clamping. */
const EDGE_MARGIN = 8;

// Right-click menu for the terminal (SPEC 7 menus, Menus board): a
// 232 wide floating surface with 6 of inner padding, 30 tall rows,
// shortcut hints on the right in the platform's glyphs, and hairline
// separators. Customize prompt… comes first on every row, since Vosh
// may not know yet which row is your prompt, and opens the prompt card
// over it (P1). Settings opens a list beside the menu, built like the
// pane menus' submenus, that goes straight to Triggers, Aliases, Macros
// and Timers, to each Settings page, and to Help. Clear scrollback is
// the one destructive item and comes last. Every action routes through
// the same path the keyboard uses (the native copy command, the input
// row's insert, the find bar, the palette's Settings links), so the
// menu never grows a second implementation. It takes focus while open
// so the arrow keys and Enter drive it, then hands focus back.
export function TerminalMenu({
  x,
  y,
  session,
  termRef,
  inputRef,
  onOpenFind,
  onCustomizePrompt,
  onClose,
}: Props) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const settingsRowRef = useRef<HTMLButtonElement | null>(null);
  const [pos, setPos] = useState({ x, y });
  const [active, setActive] = useState(-1);
  // The Settings list, and whether it opened from the keyboard and so
  // takes focus.
  const [sub, setSub] = useState<PaneSubmenuState<'settings'> | null>(null);

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
    [{ id: 'customize-prompt', label: 'Customize prompt…', run: onCustomizePrompt }],
    [
      { id: 'copy', label: 'Copy', keys: APP_SHORTCUTS.copy, run: runCopy },
      { id: 'paste', label: 'Paste', keys: 'Mod+V', run: runPaste },
      { id: 'select-all', label: 'Select all', keys: 'Mod+A', run: runSelectAll },
    ],
    [{ id: 'find', label: 'Find in scrollback…', keys: APP_SHORTCUTS.find, run: onOpenFind }],
    [{ id: 'settings', label: 'Settings', submenu: true, run: () => openSub(true) }],
    [{ id: 'clear', label: 'Clear scrollback', danger: true, run: runClear }],
  ];
  const items = groups.flat();
  const settingsAt = items.findIndex((item) => item.submenu);

  // Open the Settings list beside its row. From the keyboard or a click
  // it takes focus on its first row. Pointing at the row leaves focus
  // in the menu, and leaves a list the keyboard opened as it is.
  function openSub(focus: boolean) {
    setActive(settingsAt);
    setSub((prev) => openPaneSubmenu(prev, 'settings', focus));
  }

  // Close the list and hand focus back to the menu, so the arrow keys
  // keep working when focus was in the list.
  const closeSub = () => {
    setSub(null);
    const el = menuRef.current;
    if (el && document.activeElement !== el) el.focus({ preventScroll: true });
  };

  // Highlight a row. Any row but Settings closes the list.
  const highlight = (i: number) => {
    setActive(i);
    if (sub && !items[i]?.submenu) closeSub();
  };

  // Every item closes the menu first, then runs, the same order the
  // palette uses, so an action that moves focus wins over the menu.
  // Settings opens its list instead.
  const pick = (item: Item | undefined) => {
    if (!item) return;
    if (!item.submenu) onClose();
    item.run();
  };

  const pickRow = (row: SettingsMenuRow) => {
    onClose();
    if (row.link === null) openHelpWindow();
    else openSettingsTab(row.link);
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
      if (items[active]?.submenu) openSub(true);
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

  // The Settings list, built like the pane menus' submenus. It opens
  // right of the menu level with its row, flips left at the right edge
  // and up at the bottom edge, and Esc or ArrowLeft steps back to the
  // row. It draws apart from the menu, so its keys never reach the
  // menu's own.
  let list: ReactNode = null;
  const row = sub ? settingsRowRef.current : null;
  if (sub && row && menuRef.current) {
    list = (
      <MenuSurface
        id={SETTINGS_LIST_ID}
        label="Settings"
        nested
        autoFocus={sub.focus}
        className="pane-menu-sub"
        at={submenuAt(row.getBoundingClientRect(), menuRef.current.getBoundingClientRect())}
        onClose={() => {
          setActive(settingsAt);
          closeSub();
        }}
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
        // Settings stays the active row while its list is open, so the
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
              // The active row is the one lit. While the Settings list
              // is open, that row is Settings.
              const lit = i === active;
              return (
                <button
                  key={item.id}
                  ref={item.submenu ? settingsRowRef : undefined}
                  type="button"
                  role="menuitem"
                  tabIndex={-1}
                  aria-haspopup={item.submenu ? 'menu' : undefined}
                  aria-expanded={item.submenu ? sub !== null : undefined}
                  aria-controls={item.submenu && sub ? SETTINGS_LIST_ID : undefined}
                  className={`ov-menu-item${lit ? ' is-active' : ''}${
                    item.danger ? ' is-danger' : ''
                  }`}
                  onPointerMove={() => {
                    if (item.submenu) openSub(false);
                    else if (i !== active || sub) highlight(i);
                  }}
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
