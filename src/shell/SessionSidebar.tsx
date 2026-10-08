import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type MouseEvent,
  type PointerEvent,
} from 'react';
import type { SessionRow } from '../ipc/session';
import { useEscape } from '../lib/escapeStack';
import { sessionLabel, typedName } from '../lib/sessionLabel';
import { shortcutLabel } from '../lib/shortcuts';
import { sessionLive } from '../stores/session/connectionStore';
import { rowLook, useSessionRow } from '../stores/session/sessionRowStore';
import { CloseIcon, PlusIcon } from '../ui/icons';
import { cardWords, useCardFacts } from './cardFacts';
import { SessionCard } from './SessionCard';
import { ShellMenu, ShellMenuItem, ShellMenuSeparator } from './ShellMenu';
import { SessionMark, SessionRowBody, WaitingCount } from './SessionRowBody';
import { useHoverCard } from './useHoverCard';
import { useModHeld } from './useModHeld';
import { partShift, useRowDrag } from '../lib/useRowDrag';

// The sessions sidebar on the left of the main window, with two line
// rows. MainWindow shows it while two or more sessions are open and you
// have not hidden it in this window, and over the terminal in a window
// too narrow for its column. Its top 32 drags the window and holds the
// lights and the sessions toggle on macOS, with New session at its
// right. SESSIONS heads the list with how many are open.
//
// Each row is two lines. Line one starts with the row's status mark,
// then the session as sessionLabel names it with the port in quiet
// meta, and ends in a right column. Line two says what the session is
// doing, from useSessionLine, with your health at its right. The
// selected row is a filled pill, and the name takes the tone the row
// store gives it. The right column holds the count of what waits for
// you on a row behind. While you hold ⌘ (Ctrl elsewhere), the first
// nine rows show the key that brings each to the front there instead.
// Under the pointer a row shows its close button in that place, which
// closes its session. Rest the pointer on a row and SessionCard opens
// beside it with the rest of the session, which a screen reader hears
// as the row's description. A click selects.
//
// A right click opens the row's menu at the pointer: Rename session…,
// Edit connection…, Disconnect while the session is connected, and
// Close session. Rename session… shows F2 beside it when the row had
// the keyboard as the menu opened. A double click on the name, Return
// or F2 on a row that has the keyboard, or Rename session… from the row
// menu or anywhere else while the sidebar shows, brings the session to
// the front and turns its name into a field in place. Up and Down move
// the keyboard between rows and stop at either end, so Return or F2
// renames the row they reach. They select nothing, Space still selects,
// and F2 never reaches the command line from a row. The field spans the
// name and the right column, the mark stays, and line two says how to
// finish. Return or a click elsewhere keeps what you typed, Escape
// leaves the row as it was, and a blank field clears the name, so the
// row reads the character again.
//
// More rows than fit scroll under SESSIONS, which stays put and draws a
// hairline once a row has passed under it, and the selected row scrolls
// into view as ⌘1 to ⌘9 or a step reach it. Drag a row to move
// it, see lib/useRowDrag.
//
// WebView2 and WebKitGTK focus a button on click. Left on a row, the
// caret would take your next Space and press it again, so it goes back
// to the command line, as it does from the gear.

interface Props {
  rows: SessionRow[];
  selected: number;
  onSelect: (session: number) => void;
  onNewSession: () => void;
  /** Close a session, asking first while it is connected. */
  onClose: (session: number) => void;
  /** Put the keyboard on the selected row as the sidebar shows, as it
   *  does sliding in over the terminal. */
  takeFocus?: boolean;
  /** Hand the caret back to the command line. */
  onCaret: () => void;
  /** Keep the name typed for a session, or with null clear it. */
  onRename: (session: number, name: string | null) => void;
  /** Open the Edit connection form on the selected session. */
  onEditConnection: () => void;
  /** End a session's connection. */
  onDisconnect: (session: number) => void;
  /** Move a session to the place `to` among the other rows, from 0. */
  onMove: (session: number, to: number) => void;
}

/** What the main window asks of the sidebar. */
export interface SessionSidebarHandle {
  /** Bring `session` to the front and turn the name in its row into a
   *  field. */
  rename: (session: number) => void;
}

/** The row menu's width. */
const ROW_MENU_WIDTH = 212;

/** The rows' pitch, a 44 pill in a 46 slot. */
const ROW_PITCH = 46;

/** Whether a click left the caret on the button it pressed. */
const held = (e: MouseEvent<HTMLButtonElement>) => document.activeElement === e.currentTarget;

export const SessionSidebar = forwardRef<SessionSidebarHandle, Props>(function SessionSidebar(
  {
    rows,
    selected,
    onSelect,
    onNewSession,
    onClose,
    takeFocus = false,
    onCaret,
    onRename,
    onEditConnection,
    onDisconnect,
    onMove,
  },
  ref,
) {
  const numbered = useModHeld();
  const [side, setSide] = useState<HTMLElement | null>(null);
  const list = useRef<HTMLUListElement | null>(null);
  // Whether a row has passed under SESSIONS, which draws its hairline.
  const [scrolled, setScrolled] = useState(false);
  const { drag, press, dropped } = useRowDrag(
    list,
    rows.map((row) => row.id),
    onMove,
    ROW_PITCH,
  );
  const card = useHoverCard(drag !== null);
  // Each row's button, by session, for Up and Down. A row being renamed
  // has none.
  const buttons = useRef(new Map<number, HTMLButtonElement>());
  const step = (from: number, delta: number) => {
    const to = rows[from + delta];
    if (to) buttons.current.get(to.id)?.focus();
  };
  // The session whose name is a field, and the row whose menu is open,
  // with the pointer it opened at and whether the row had the keyboard.
  const [renaming, setRenaming] = useState<number | null>(null);
  const [menu, setMenu] = useState<{
    session: number;
    x: number;
    y: number;
    keyed: boolean;
  } | null>(null);
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

  // The selected row scrolls into view, whatever brought it to the
  // front.
  const at = rows.findIndex((row) => row.id === selected);
  useEffect(() => {
    const el = list.current;
    if (!el || at < 0) return;
    const top = at * ROW_PITCH;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + ROW_PITCH > el.scrollTop + el.clientHeight)
      el.scrollTop = top + ROW_PITCH - el.clientHeight;
  }, [at]);

  // Sliding in over the terminal, the sidebar takes the keyboard on the
  // selected row, so Up, Down and Space pick a session at once.
  useEffect(() => {
    if (!takeFocus) return;
    const first = rows[0];
    const row = buttons.current.get(selected) ?? (first && buttons.current.get(first.id));
    row?.focus();
    // On mount alone, as the sidebar slides in.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const closeMenu = () => {
    setMenu(null);
    onCaret();
  };
  const fromMenu = (action: () => void) => {
    closeMenu();
    action();
  };

  return (
    <aside ref={setSide} className="shell-sessions st-controls" aria-label="Sessions">
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
        </div>
      </div>
      <h2 className={scrolled ? 'shell-sessions-head is-scrolled' : 'shell-sessions-head'}>
        Sessions<span className="shell-sessions-total">{rows.length}</span>
      </h2>
      <ul
        ref={list}
        className={drag ? 'shell-sessions-list is-dragging' : 'shell-sessions-list'}
        onScroll={(e) => {
          setScrolled(e.currentTarget.scrollTop > 0);
          card.leave();
        }}
        onPointerLeave={card.leave}
      >
        {rows.map((row, i) => (
          <SessionSlot
            key={row.id}
            row={row}
            rows={rows}
            current={row.id === selected}
            keys={numbered && i < 9 ? shortcutLabel(`Mod+${i + 1}`) : null}
            renaming={row.id === renaming}
            lifted={drag?.id === row.id}
            offset={
              drag
                ? drag.id === row.id
                  ? drag.dy
                  : partShift(i, drag.from, drag.to, ROW_PITCH)
                : 0
            }
            onPress={(e) => press(e, row.id)}
            onButton={(el) => {
              if (el) buttons.current.set(row.id, el);
              else buttons.current.delete(row.id);
            }}
            onStep={(delta) => step(i, delta)}
            onRest={(slot) => card.rest(row.id, slot)}
            card={card.shown?.session === row.id ? { slot: card.shown.slot, side } : null}
            dropped={dropped}
            onSelect={onSelect}
            onClose={onClose}
            onCaret={onCaret}
            onMenu={(x, y, keyed) => setMenu({ session: row.id, x, y, keyed })}
            onRename={() => startRename(row.id)}
            onRenamed={(name, caret) => renamed(row.id, name, caret)}
          />
        ))}
        {/* The line where the row in the air lands. */}
        {drag && (
          <li
            className="shell-sessions-drop"
            aria-hidden="true"
            style={{ top: drag.to * ROW_PITCH }}
          />
        )}
      </ul>
      {menu && menuRow && (
        <ShellMenu at={menu} width={ROW_MENU_WIDTH} label="Session options" onClose={closeMenu}>
          <ShellMenuItem
            shortcut={menu.keyed ? 'F2' : undefined}
            onSelect={() => fromMenu(() => startRename(menuRow.id))}
          >
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

interface SlotProps {
  row: SessionRow;
  rows: SessionRow[];
  current: boolean;
  /** The key that brings this row to the front, shown while you hold
   *  Mod, or null. */
  keys: string | null;
  /** Its name is a field while you rename it. */
  renaming: boolean;
  /** It is the row in the air. */
  lifted: boolean;
  /** How far it sits from its place while a row is in the air. */
  offset: number;
  /** A press that may lift the row. */
  onPress: (e: PointerEvent<HTMLButtonElement>) => void;
  /** Hands the sidebar the row's button, or null as it goes. */
  onButton: (el: HTMLButtonElement | null) => void;
  /** Move the keyboard `delta` rows down, or up when negative. */
  onStep: (delta: number) => void;
  /** The pointer moved on the row's slot. */
  onRest: (slot: HTMLElement) => void;
  /** Where the row's card shows while it does, level with its slot and
   *  right of the sidebar, else null. */
  card: { slot: HTMLElement; side: HTMLElement | null } | null;
  /** Whether the click under way ends a drag, and selects nothing. */
  dropped: () => boolean;
  onSelect: (session: number) => void;
  onClose: (session: number) => void;
  onCaret: () => void;
  /** Open the row's menu at the pointer, and say whether the row had
   *  the keyboard. */
  onMenu: (x: number, y: number, keyed: boolean) => void;
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
  lifted,
  offset,
  onPress,
  onButton,
  onStep,
  onRest,
  card,
  dropped,
  onSelect,
  onClose,
  onCaret,
  onMenu,
  onRename,
  onRenamed,
}: SlotProps) {
  const label = sessionLabel(row, rows);
  const look = rowLook(useSessionRow(row.id), row, current);
  const facts = useCardFacts(row, rows);
  const described = `shell-sessions-card-${row.id}`;
  const rowClass = look.tone ? `shell-sessions-row is-${look.tone}` : 'shell-sessions-row';
  const moved = offset ? { transform: `translateY(${offset}px)` } : undefined;

  // A field cannot sit inside a button, so the row is a plain box while
  // you type, and it has no close button then.
  if (renaming) {
    return (
      <li className="shell-sessions-slot" style={moved}>
        <div className={`${rowClass} is-edit`} aria-current={current ? 'true' : undefined}>
          <SessionMark mark={look.mark} />
          <NameField
            initial={label.name}
            unnamed={sessionLabel({ ...row, name: null }, rows).name}
            onDone={onRenamed}
          />
          <span className="shell-sessions-line is-hint">Return saves, Esc cancels</span>
        </div>
      </li>
    );
  }

  return (
    <li
      className={lifted ? 'shell-sessions-slot is-lifted' : 'shell-sessions-slot'}
      style={moved}
      onPointerMove={(e) => onRest(e.currentTarget)}
    >
      <button
        ref={onButton}
        type="button"
        className={rowClass}
        aria-current={current ? 'true' : undefined}
        aria-describedby={described}
        onPointerDown={onPress}
        onClick={() => {
          if (dropped()) return;
          onSelect(row.id);
          // Picking a session puts you back on its command line.
          onCaret();
        }}
        onKeyDown={(e) => {
          if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey || e.nativeEvent.isComposing) return;
          if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            e.preventDefault();
            onStep(e.key === 'ArrowUp' ? -1 : 1);
            return;
          }
          if (e.key !== 'Enter' && e.key !== 'F2') return;
          e.preventDefault();
          e.stopPropagation();
          onRename();
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          onMenu(e.clientX, e.clientY, e.currentTarget.matches(':focus-visible'));
        }}
      >
        <SessionRowBody
          row={row}
          rows={rows}
          mark={look.mark}
          end={
            keys ? (
              <span className="shell-sessions-key">{keys}</span>
            ) : (
              look.count > 0 && !current && <WaitingCount count={look.count} />
            )
          }
          onNameDoubleClick={onRename}
        />
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
      <span id={described} hidden>
        {cardWords(facts)}
      </span>
      {card?.side && <SessionCard facts={facts} slot={card.slot} side={card.side} />}
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
