import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { nativeSurfaceEnabled, type TerminalHandle } from './Terminal';
import { type InputHandle } from './Input';
import { disconnectSession } from '../lib/session';
import { toggleWellSplits, wellSplitsOpen } from '../lib/wellSplits';

interface Props {
  /** Pointer position in viewport coordinates. The menu opens with its
   *  top-left corner here, clamped so it never overflows the window. */
  x: number;
  y: number;
  connected: boolean;
  termRef: RefObject<TerminalHandle | null>;
  inputRef: RefObject<InputHandle | null>;
  onOpenFind: () => void;
  onClose: () => void;
}

/** How close the menu may sit to the window edge after clamping. */
const EDGE_MARGIN = 8;

// Right-click context menu for the terminal area, from the Ember
// Menus canvas: 232px floating surface, icon + label + mono shortcut
// rows, hairline separators, destructive disconnect at the bottom.
// Every action routes through the same paths the keyboard uses (the
// native-surface copy command, the input row's insert, the find
// toolbar) so the menu never grows a second implementation. Closes on
// outside pointerdown, Escape, or any item click.
export function TerminalMenu({ x, y, connected, termRef, inputRef, onOpenFind, onClose }: Props) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const [pos, setPos] = useState({ x, y });
  const splitsOpen = wellSplitsOpen();

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

  // Outside-click + Escape close. Pointerdown so the close fires
  // before any click handler inside an unrelated element.
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

  // Every item closes the menu first, then runs — same order the
  // palette uses, so an action that moves focus wins over the menu.
  const pick = (run: () => void) => () => {
    onClose();
    run();
  };

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

  const runPaste = () => {
    void navigator.clipboard
      .readText()
      .then((text) => {
        if (text.length > 0) inputRef.current?.insert(text);
      })
      .catch(() => {});
  };

  return (
    <div
      ref={menuRef}
      className="terminal-menu"
      role="menu"
      aria-label="terminal menu"
      data-occludes-surface="true"
      style={{ left: pos.x, top: pos.y }}
    >
      <button type="button" role="menuitem" className="terminal-menu-item" onClick={pick(runCopy)}>
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
          <rect x="4.5" y="4.5" width="7.5" height="7.5" rx="1" />
          <path d="M9.5 2.5H3.5a1 1 0 0 0-1 1v6" />
        </svg>
        <span className="terminal-menu-label">copy</span>
        <span className="terminal-menu-shortcut">&#8984;C</span>
      </button>
      <button type="button" role="menuitem" className="terminal-menu-item" onClick={pick(runPaste)}>
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
          <rect x="2.5" y="2.8" width="9" height="9.7" rx="1" />
          <rect x="5" y="1.3" width="4" height="2.6" rx="0.6" />
        </svg>
        <span className="terminal-menu-label">paste</span>
        <span className="terminal-menu-shortcut">&#8984;V</span>
      </button>
      <div className="terminal-menu-sep" />
      <button
        type="button"
        role="menuitem"
        className="terminal-menu-item"
        onClick={pick(toggleWellSplits)}
      >
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
          <rect x="1.5" y="2.5" width="11" height="9" rx="1" />
          <path d="M7 2.5v9" />
        </svg>
        <span className="terminal-menu-label">{splitsOpen ? 'close splits' : 'open splits'}</span>
      </button>
      <div className="terminal-menu-sep" />
      <button
        type="button"
        role="menuitem"
        className="terminal-menu-item"
        onClick={pick(onOpenFind)}
      >
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
          <circle cx="6.2" cy="6.2" r="3.7" />
          <path d="M9 9l3.2 3.2" />
        </svg>
        <span className="terminal-menu-label">search scrollback</span>
        <span className="terminal-menu-shortcut">&#8984;F</span>
      </button>
      {/* Clear only reaches the xterm buffer; the native grid has no
          clear command, so the item hides where it would visibly no-op. */}
      {!nativeSurfaceEnabled() && (
        <button
          type="button"
          role="menuitem"
          className="terminal-menu-item"
          onClick={pick(() => termRef.current?.clear())}
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
            <circle cx="7" cy="7" r="5" />
            <path d="M3.6 10.4 10.4 3.6" />
          </svg>
          <span className="terminal-menu-label">clear buffer</span>
        </button>
      )}
      {connected && (
        <>
          <div className="terminal-menu-sep" />
          <button
            type="button"
            role="menuitem"
            className="terminal-menu-item terminal-menu-item-danger"
            onClick={pick(() => void disconnectSession())}
          >
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <path d="M7 1.6v4.8" />
              <path d="M4.2 3.6a4.6 4.6 0 1 0 5.6 0" />
            </svg>
            <span className="terminal-menu-label">disconnect</span>
          </button>
        </>
      )}
    </div>
  );
}
