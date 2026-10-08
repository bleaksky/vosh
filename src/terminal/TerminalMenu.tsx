import { Fragment, useEffect, useRef, useState, type ReactNode, type RefObject } from 'react';
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
   *  top-left corner here, kept inside the window. */
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

/** The lists a row opens beside the menu. */
type Sub = 'write' | 'settings';

/** The id of the list each row controls. */
const LIST_ID: Record<Sub, string> = {
  write: 'terminal-menu-write',
  settings: 'terminal-menu-settings',
};

// Right-click menu for the terminal, drawn by the one menu surface with
// shortcuts on the right in the platform's glyphs. Customize prompt…
// comes first on every row, since Vosh may not know yet which row is
// your prompt, and opens the prompt card over it. Write under it opens
// the kinds of text the writing card takes. Settings opens a list beside the menu, built
// like the pane menus' submenus, that goes straight to Triggers,
// Aliases, Macros and Timers, to each Settings page, and to Help. Clear
// scrollback is the one destructive item and comes last. Every action
// routes through the same path the keyboard uses (the native copy
// command, the input row's insert, the find bar, the palette's Settings
// links), so the menu never grows a second implementation. It opens
// with focus on the menu and no row lit, so the arrow keys and Enter
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
  const subRows = useRef<Partial<Record<Sub, HTMLButtonElement | null>>>({});
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

  // Hand focus back to where it was when the menu closes without
  // moving it somewhere else.
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    return () => {
      const current = document.activeElement;
      if (!current || current === document.body) previous?.focus();
    };
  }, []);

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

  // Every row closes the menu first, then runs, the same order the
  // palette uses, so an action that moves focus wins over the menu.
  const pick = (run: () => void) => () => {
    onClose();
    run();
  };

  // Pointing at a row, or the arrow keys landing on it, closes a list
  // the row does not open.
  const closeSub = () => setSub(null);
  const plain = { onHover: closeSub, onFocus: closeSub };

  /** A row that opens `which` beside the menu. Pointing at it opens the
   *  list and leaves focus on the row, where ArrowLeft shuts it.
   *  ArrowRight, Enter, Space or a click open it with focus on its
   *  first row. */
  const subRow = (which: Sub, label: string) => (
    <MenuItem
      itemRef={(el) => {
        subRows.current[which] = el;
      }}
      onFocus={() => setSub((prev) => (prev && prev.which !== which ? null : prev))}
      submenu={{
        open: sub?.which === which,
        controls: LIST_ID[which],
        onOpen: (focus) => setSub((prev) => openPaneSubmenu(prev, which, focus)),
        onClose: closeSub,
      }}
      trailing={<ChevronRightIcon className="menu-chevron" />}
    >
      {label}
    </MenuItem>
  );

  // The list beside its row, built like the pane menus' submenus. It
  // opens right of the menu level with its row, flips left at the right
  // edge and up at the bottom edge, and Esc or ArrowLeft steps back to
  // the row.
  let list: ReactNode = null;
  const row = sub ? subRows.current[sub.which] : null;
  if (sub && row) {
    const which = sub.which;
    const r = row.getBoundingClientRect();
    const at = submenuAt(r, row.closest('menu')?.getBoundingClientRect() ?? r);
    const back = () => {
      setSub(null);
      subRows.current[which]?.focus();
    };
    list =
      which === 'write' ? (
        <MenuSurface
          id={LIST_ID.write}
          label="Write"
          nested
          autoFocus={sub.focus}
          className="menu-sub"
          at={at}
          onClose={back}
        >
          {writeKinds.map((kind) => (
            <MenuItem key={kind} onSelect={pick(() => onWrite(kind))}>
              {KINDS[kind].menu}
            </MenuItem>
          ))}
          <MenuSeparator />
          <MenuItem onSelect={pick(() => onWrite('description'))}>
            {KINDS.description.menu}
          </MenuItem>
          <MenuItem onSelect={pick(() => onWrite('history'))}>{KINDS.history.menu}</MenuItem>
        </MenuSurface>
      ) : (
        <MenuSurface
          id={LIST_ID.settings}
          label="Settings"
          nested
          autoFocus={sub.focus}
          className="menu-sub"
          at={at}
          onClose={back}
        >
          {SETTINGS_MENU.map((group, g) => (
            <Fragment key={group[0].id}>
              {g > 0 && <MenuSeparator />}
              {group.map((entry) => (
                <MenuItem
                  key={entry.id}
                  {...(entry.keys !== undefined && { keys: entry.keys })}
                  onSelect={pick(() => openRow(entry))}
                >
                  {entry.label}
                </MenuItem>
              ))}
            </Fragment>
          ))}
        </MenuSurface>
      );
  }

  return (
    <>
      <MenuSurface label="Terminal" at={{ x, y }} focus="surface" onClose={onClose}>
        <MenuItem {...plain} onSelect={pick(onCustomizePrompt)}>
          Customize prompt…
        </MenuItem>
        {subRow('write', 'Write')}
        <MenuSeparator />
        <MenuItem {...plain} keys={APP_SHORTCUTS.copy} onSelect={pick(runCopy)}>
          Copy
        </MenuItem>
        <MenuItem {...plain} keys="Mod+V" onSelect={pick(runPaste)}>
          Paste
        </MenuItem>
        <MenuItem {...plain} keys="Mod+A" onSelect={pick(runSelectAll)}>
          Select all
        </MenuItem>
        <MenuSeparator />
        <MenuItem {...plain} keys={APP_SHORTCUTS.find} onSelect={pick(onOpenFind)}>
          Find in scrollback…
        </MenuItem>
        <MenuItem
          {...plain}
          disabled={!logged}
          onSelect={pick(() => openSettingsTab('logs:scene'))}
        >
          Save a scene…
        </MenuItem>
        <MenuSeparator />
        {subRow('settings', 'Settings')}
        <MenuSeparator />
        <MenuItem {...plain} danger onSelect={pick(runClear)}>
          Clear scrollback
        </MenuItem>
      </MenuSurface>
      {list}
    </>
  );
}

function openRow(row: SettingsMenuRow) {
  if (row.link === null) openHelpWindow();
  else openSettingsTab(row.link);
}
