import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type ComponentType,
  type MouseEvent,
} from 'react';
import type { SessionRow } from '../ipc/session';
import { useEscape } from '../lib/escapeStack';
import { sessionLabel, typedName } from '../lib/sessionLabel';
import { shortcutLabel } from '../lib/shortcuts';
import { sessionLive } from '../stores/session/connectionStore';
import { rowLook, useSessionRow, type RowGlyph } from '../stores/session/sessionRowStore';
import { CloseIcon, DotIcon, HandIcon, PlusIcon, SpinnerIcon, TriangleIcon } from '../ui/icons';
import { SidebarIcon } from './icons';
import { ShellMenu, ShellMenuItem, ShellMenuSeparator } from './ShellMenu';
import { useModHeld } from './useModHeld';

// The sessions sidebar on the left of the main window, board 2 of the
// Sessions review, drawn to otty's measures (Q17). MainWindow shows it
// while two or more sessions are open and you have not hidden it in this
// window. Its top 32 drags the window and holds the lights on macOS, with
// New session and Hide sessions at its right. SESSIONS heads the list,
// and each row reads the session as sessionLabel names it, the selected
// one a filled pill. A row's glyph takes the meta's place while it shows,
// and its name takes the tone the row store gives it (board 3). A click
// selects. Under the pointer a row shows its close button in the place
// of the meta or the glyph, which closes its session (Q13). While you
// hold ⌘ (Ctrl elsewhere), the first nine rows show the key that brings
// each to the front in that place instead, as otty does (board 8).
//
// A right click opens the row's menu at the pointer, board 9: Rename
// session…, Edit connection…, Disconnect while the session is
// connected, and Close session. Rename session…, a double click on the
// name, or Rename session… from anywhere else while the sidebar shows,
// brings the session to the front and turns its name into a field in
// place (Q7). Return or a click elsewhere keeps what you typed, Escape
// leaves the row as it was, and a blank field clears the name, so the
// row reads the character again.
//
// WebView2 and WebKitGTK focus a button on click. Left on a row or Hide
// sessions, the caret would take your next Space and press it again, so
// it goes back to the command line, as it does from the gear.

interface Props {
  rows: SessionRow[];
  selected: number;
  onSelect: (session: number) => void;
  onNewSession: () => void;
  /** Close a session, asking first while it is connected. */
  onClose: (session: number) => void;
  /** Fold the sidebar away in this window. */
  onHide: () => void;
  /** Hand the caret back to the command line. */
  onCaret: () => void;
  /** Keep the name typed for a session, or with null clear it. */
  onRename: (session: number, name: string | null) => void;
  /** Open the Edit connection form on the selected session. */
  onEditConnection: () => void;
  /** End a session's connection. */
  onDisconnect: (session: number) => void;
}

/** What the main window asks of the sidebar. */
export interface SessionSidebarHandle {
  /** Bring `session` to the front and turn the name in its row into a
   *  field. */
  rename: (session: number) => void;
}

/** The row menu's width, as board 9 draws it. */
const ROW_MENU_WIDTH = 212;

/** Whether a click left the caret on the button it pressed. */
const held = (e: MouseEvent<HTMLButtonElement>) => document.activeElement === e.currentTarget;

export const SessionSidebar = forwardRef<SessionSidebarHandle, Props>(function SessionSidebar(
  {
    rows,
    selected,
    onSelect,
    onNewSession,
    onClose,
    onHide,
    onCaret,
    onRename,
    onEditConnection,
    onDisconnect,
  },
  ref,
) {
  const numbered = useModHeld();
  // The session whose name is a field, and the row whose menu is open,
  // with the pointer it opened at.
  const [renaming, setRenaming] = useState<number | null>(null);
  const [menu, setMenu] = useState<{ session: number; x: number; y: number } | null>(null);
  const menuRow = menu && rows.find((row) => row.id === menu.session);

  // Every Rename session… acts on the session in front, so a row behind
  // comes to the front first.
  const startRename = (session: number) => {
    if (session !== selected) onSelect(session);
    setRenaming(session);
  };
  useImperativeHandle(ref, () => ({ rename: startRename }));

  /** The field closed, keeping `name` unless it is undefined. Return and
   *  Escape hand the caret back, and a click elsewhere leaves it there. */
  const renamed = (session: number, name: string | null | undefined, caret: boolean) => {
    setRenaming(null);
    if (name !== undefined) onRename(session, name);
    if (caret) onCaret();
  };

  const closeMenu = () => {
    setMenu(null);
    onCaret();
  };
  const fromMenu = (action: () => void) => {
    closeMenu();
    action();
  };

  return (
    <aside className="shell-sessions st-controls" aria-label="Sessions">
      <div className="shell-sessions-top" data-tauri-drag-region>
        <div className="shell-sessions-actions">
          <button
            type="button"
            className="shell-icon-button"
            aria-label="New session"
            onClick={onNewSession}
          >
            <PlusIcon />
          </button>
          <button
            type="button"
            className="shell-icon-button"
            aria-label="Hide sessions"
            onClick={(e) => {
              const caret = held(e);
              onHide();
              if (caret) onCaret();
            }}
          >
            <SidebarIcon />
          </button>
        </div>
      </div>
      <h2 className="shell-sessions-head">Sessions</h2>
      <ul className="shell-sessions-list">
        {rows.map((row, i) => (
          <SessionSlot
            key={row.id}
            row={row}
            rows={rows}
            current={row.id === selected}
            keys={numbered && i < 9 ? shortcutLabel(`Mod+${i + 1}`) : null}
            renaming={row.id === renaming}
            onSelect={onSelect}
            onClose={onClose}
            onCaret={onCaret}
            onMenu={(x, y) => setMenu({ session: row.id, x, y })}
            onRename={() => startRename(row.id)}
            onRenamed={(name, caret) => renamed(row.id, name, caret)}
          />
        ))}
      </ul>
      {menu && menuRow && (
        <ShellMenu at={menu} width={ROW_MENU_WIDTH} label="Session options" onClose={closeMenu}>
          <ShellMenuItem onSelect={() => fromMenu(() => startRename(menuRow.id))}>
            Rename session…
          </ShellMenuItem>
          <ShellMenuItem
            onSelect={() =>
              fromMenu(() => {
                if (menuRow.id !== selected) onSelect(menuRow.id);
                onEditConnection();
              })
            }
          >
            Edit connection…
          </ShellMenuItem>
          <ShellMenuSeparator />
          {(menuRow.connected || sessionLive(menuRow.id)) && (
            <ShellMenuItem onSelect={() => fromMenu(() => onDisconnect(menuRow.id))}>
              Disconnect
            </ShellMenuItem>
          )}
          <ShellMenuItem onSelect={() => fromMenu(() => onClose(menuRow.id))}>
            Close session
          </ShellMenuItem>
        </ShellMenu>
      )}
    </aside>
  );
});

/** Each glyph, with the words a screen reader says for it, from board
 *  3 and Q8, where the triangle means connect again yourself. */
const GLYPHS: Record<RowGlyph, { icon: ComponentType; words: string }> = {
  triangle: { icon: TriangleIcon, words: 'Connect again' },
  hand: { icon: HandIcon, words: 'Logging in' },
  spinner: { icon: SpinnerIcon, words: 'Connecting' },
  dot: { icon: DotIcon, words: 'Something for you' },
};

interface SlotProps {
  row: SessionRow;
  rows: SessionRow[];
  current: boolean;
  /** The key that brings this row to the front, shown while you hold
   *  Mod, or null. */
  keys: string | null;
  /** Its name is a field while you rename it. */
  renaming: boolean;
  onSelect: (session: number) => void;
  onClose: (session: number) => void;
  onCaret: () => void;
  /** Open the row's menu at the pointer. */
  onMenu: (x: number, y: number) => void;
  onRename: () => void;
  /** The name field closed, with the name to keep, or undefined to keep
   *  the row as it was. */
  onRenamed: (name: string | null | undefined, caret: boolean) => void;
}

/** One session's row. */
function SessionSlot({
  row,
  rows,
  current,
  keys,
  renaming,
  onSelect,
  onClose,
  onCaret,
  onMenu,
  onRename,
  onRenamed,
}: SlotProps) {
  const label = sessionLabel(row, rows);
  const { glyph, tone } = rowLook(useSessionRow(row.id), row, current);
  const look = tone ? `shell-sessions-row is-${tone}` : 'shell-sessions-row';
  const meta = keys ? (
    <span className="shell-sessions-meta is-key">{keys}</span>
  ) : glyph ? (
    <RowGlyphMark glyph={glyph} />
  ) : (
    label.meta && <span className="shell-sessions-meta">{label.meta}</span>
  );

  // A field cannot sit inside a button, so the row is a plain box while
  // you type, and it has no close button then.
  if (renaming) {
    return (
      <li className="shell-sessions-slot">
        <div className={`${look} is-edit`} aria-current={current ? 'true' : undefined}>
          <NameField
            initial={label.name}
            unnamed={sessionLabel({ ...row, name: null }, rows).name}
            onDone={onRenamed}
          />
          {meta}
        </div>
      </li>
    );
  }

  return (
    <li className="shell-sessions-slot">
      <button
        type="button"
        className={look}
        aria-current={current ? 'true' : undefined}
        title={label.tooltip ?? undefined}
        onClick={(e) => {
          onSelect(row.id);
          if (held(e)) onCaret();
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          onMenu(e.clientX, e.clientY);
        }}
      >
        {label.split ? (
          <span className="shell-sessions-name is-world" onDoubleClick={onRename}>
            <span className="shell-sessions-world">{label.split.world}</span>
            {label.split.port}
          </span>
        ) : (
          <span className="shell-sessions-name" onDoubleClick={onRename}>
            {label.name}
          </span>
        )}
        {meta}
      </button>
      {/* A sibling of the row, since a button holds no button. It shows
          only under the pointer, so Tab passes it by, and the Session
          menu and the palette close a session from the keyboard. The
          caret goes back first, so a question that asks returns it
          there. */}
      <button
        type="button"
        className="shell-sessions-close"
        aria-label="Close session"
        tabIndex={-1}
        onClick={(e) => {
          if (held(e)) onCaret();
          onClose(row.id);
        }}
      >
        <CloseIcon />
      </button>
    </li>
  );
}

interface NameFieldProps {
  /** What the row read as the field opened, the text it starts with. */
  initial: string;
  /** What the row reads with no name, which a blank field shows. */
  unnamed: string;
  onDone: (name: string | null | undefined, caret: boolean) => void;
}

/** A session's name typed in place of it, at the name's own x with its
 *  text selected. Return keeps what you typed, Escape keeps the row as
 *  it was, and leaving the field keeps what you typed too. */
function NameField({ initial, unnamed, onDone }: NameFieldProps) {
  const [text, setText] = useState(initial);
  const ref = useRef<HTMLInputElement | null>(null);
  // Set once the field closed, so the blur its own close causes keeps
  // nothing a second time.
  const done = useRef(false);
  const finish = (name: string | null | undefined, caret: boolean) => {
    if (done.current) return;
    done.current = true;
    onDone(name, caret);
  };

  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);

  useEscape(true, () => finish(undefined, true));

  return (
    <input
      ref={ref}
      className="shell-sessions-field"
      type="text"
      value={text}
      placeholder={unnamed}
      aria-label="Session name"
      spellCheck={false}
      autoComplete="off"
      autoCorrect="off"
      autoCapitalize="off"
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => {
        if (e.key !== 'Enter' || e.nativeEvent.isComposing) return;
        e.preventDefault();
        finish(typedName(text, initial), true);
      }}
      onBlur={() => finish(typedName(text, initial), false)}
    />
  );
}

function RowGlyphMark({ glyph }: { glyph: RowGlyph }) {
  const { icon: Icon, words } = GLYPHS[glyph];
  return (
    <span className={`shell-sessions-glyph is-${glyph}`} role="img" aria-label={words}>
      <Icon />
    </span>
  );
}
