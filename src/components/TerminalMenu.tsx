import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { nativeSurfaceEnabled, type TerminalHandle } from './Terminal';
import { type InputHandle } from './Input';
import APP_SHORTCUTS from '../lib/appShortcuts.json';
import { shortcutLabel } from '../lib/palette';

interface Props {
  /** Pointer position in viewport coordinates. The menu opens with its
   *  top-left corner here, clamped so it never overflows the window. */
  x: number;
  y: number;
  /** Unused since the session controls moved to the title band. Kept
   *  so existing call sites still type check. */
  connected?: boolean;
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
  run: () => void;
}

/** How close the menu may sit to the window edge after clamping. */
const EDGE_MARGIN = 8;

// Right-click menu for the terminal (SPEC 7 menus, Menus board): a
// 232 wide floating surface with 6 of inner padding, 30 tall rows,
// shortcut hints on the right in the platform's glyphs, and hairline
// separators. Customize prompt… comes first on every row, since Vosh
// may not know yet which row is your prompt, and opens the prompt card
// over it (P1). Clear scrollback is the one destructive item and comes
// last. Every action routes through the same path the keyboard uses
// (the native copy command, the input row's insert, the find bar), so
// the menu never grows a second implementation. It takes focus while
// open so the arrow keys and Enter drive it, then hands focus back.
export function TerminalMenu({
  x,
  y,
  termRef,
  inputRef,
  onOpenFind,
  onCustomizePrompt,
  onClose,
}: Props) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const [pos, setPos] = useState({ x, y });
  const [active, setActive] = useState(-1);

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
  // before any click handler inside an unrelated element. Escape also
  // closes when focus has drifted out of the menu.
  useEffect(() => {
    const onPointer = (e: PointerEvent) => {
      if (!menuRef.current) return;
      if (e.target instanceof Node && menuRef.current.contains(e.target)) return;
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
      void invoke('native_surface_copy').catch(() => {});
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
      void invoke('native_surface_select_all').catch(() => {});
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

  // Groups split by separators. Clear only reaches the xterm buffer.
  // The native grid has no clear command, so the item hides where it
  // would visibly do nothing.
  const groups: Item[][] = [
    [{ id: 'customize-prompt', label: 'Customize prompt…', run: onCustomizePrompt }],
    [
      { id: 'copy', label: 'Copy', keys: APP_SHORTCUTS.copy, run: runCopy },
      { id: 'paste', label: 'Paste', keys: 'Mod+V', run: runPaste },
      { id: 'select-all', label: 'Select all', keys: 'Mod+A', run: runSelectAll },
    ],
    [{ id: 'find', label: 'Find in scrollback…', keys: APP_SHORTCUTS.find, run: onOpenFind }],
  ];
  if (!nativeSurfaceEnabled()) {
    groups.push([
      {
        id: 'clear',
        label: 'Clear scrollback',
        danger: true,
        run: () => termRef.current?.clear(),
      },
    ]);
  }
  const items = groups.flat();

  // Every item closes the menu first, then runs, the same order the
  // palette uses, so an action that moves focus wins over the menu.
  const pick = (item: Item | undefined) => {
    if (!item) return;
    onClose();
    item.run();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActive((i) => (i + 1) % items.length);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActive((i) => (i <= 0 ? items.length - 1 : i - 1));
    } else if (e.key === 'Home') {
      e.preventDefault();
      setActive(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      setActive(items.length - 1);
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      pick(items[active]);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    } else if (e.key === 'Tab') {
      e.preventDefault();
    }
  };

  let index = -1;
  return (
    <div
      ref={menuRef}
      className="ov-menu ov-terminal-menu"
      role="menu"
      aria-label="Terminal"
      tabIndex={-1}
      data-occludes-surface="true"
      style={{ left: pos.x, top: pos.y }}
      onKeyDown={onKeyDown}
      onPointerLeave={() => setActive(-1)}
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
            return (
              <button
                key={item.id}
                type="button"
                role="menuitem"
                tabIndex={-1}
                className={`ov-menu-item${i === active ? ' is-active' : ''}${
                  item.danger ? ' is-danger' : ''
                }`}
                onPointerMove={() => {
                  if (i !== active) setActive(i);
                }}
                onClick={() => pick(item)}
              >
                <span className="ov-menu-label">{item.label}</span>
                {item.keys && <kbd className="ov-menu-keys">{shortcutLabel(item.keys)}</kbd>}
              </button>
            );
          })}
        </div>
      ))}
    </div>
  );
}
