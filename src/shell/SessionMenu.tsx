import { useState } from 'react';
import type { SessionRow } from '../ipc/session';
import APP_SHORTCUTS from '../lib/appShortcuts.json';
import type { SessionMenuRequest } from '../lib/appMenu';
import { shortcutLabel } from '../lib/shortcuts';
import { worldName } from '../lib/knownWorlds';
import { rowLook, useSessionRow } from '../stores/session/sessionRowStore';
import { returnToCommandLine } from '../panel/paneActions';
import { goTo, useSelected, useSessions } from '../stores/session/sessionsStore';
import type { Connection } from '../stores/session/useConnection';
import { CheckIcon, CloseIcon } from '../ui/icons';
import { ConnectionForm } from './ConnectionForm';
import { openNewSession } from './newSession';
import { NewSessionForm } from './NewSessionForm';
import { RenameSessionForm } from './RenameSessionForm';
import { SessionRowBody, WaitingCount } from './SessionRowBody';
import { ShellMenu, ShellMenuItem, ShellMenuSeparator } from './ShellMenu';

// The session popover under the title button, by board 4 of the
// Sessions review: Connect to the selected session's world or
// Disconnect, Edit connection, Rename session, and New session. Edit
// connection swaps the list for a host, port and TLS form in the same
// popover. Rename session turns the selected row's name into a field
// while the sessions sidebar shows (board 9), and swaps in a form with
// one Name field while it does not, as with one session. New session
// opens a session and comes back on that session's own form. Disconnect
// is destructive, so it sits last in the danger tone and is never the
// row focus lands on.
//
// While the sidebar is folded with two or more sessions open, in a
// narrow window or after Hide sessions, the popover lists every session
// at its top under SESSIONS, board 8, in the sidebar's two line rows
// (S7 of the Sessions Sidebar review, board 05). The selected one wears
// the check in the right column, a session behind the count of what
// waits there, and any other one the key that brings it to the front. A
// click brings that session to the front. The list takes the sidebar's
// place, so under the pointer each row shows the close button in the
// right column, which closes that session as the sidebar's does (Q13). A
// list too long for the window scrolls, and the rows under it stay put.

const MENU_WIDTH = 272;

interface Props {
  connection: Connection;
  anchor: HTMLElement | null;
  /** What the popover opens on. The macOS menu bar's Edit connection
   *  opens it on its form, and Cancel there closes it instead of
   *  stepping back to a list you never saw. Rename session… from the
   *  menu bar or the palette with no sidebar does the same with its
   *  form. New session… opens it on the form of the session it
   *  opened. */
  request?: SessionMenuRequest;
  /** Turn the selected session's name into a field in its row, while
   *  the sessions sidebar shows. */
  renameInRow?: (() => void) | undefined;
  /** List every session at the top, while the sidebar is folded. */
  listSessions?: boolean;
  /** Close a session from the list, asking first while it is
   *  connected. */
  onCloseSession?: ((session: number) => void) | undefined;
  onClose: () => void;
}

export function SessionMenu({
  connection,
  anchor,
  request = { mode: 'menu' },
  renameInRow,
  listSessions = false,
  onCloseSession,
  onClose,
}: Props) {
  const [mode, setMode] = useState(request.mode);
  const { live, target } = connection;
  const rows = useSessions();
  const selected = useSelected();

  const run = (action: () => Promise<void> | void) => {
    onClose();
    void action();
  };
  // Picking a session puts you back on its command line, once the
  // popover has handed focus back.
  const pick = (id: number) => {
    goTo(id);
    setTimeout(returnToCommandLine, 0);
  };

  if (request.mode === 'new') {
    return (
      <ShellMenu
        anchor={anchor}
        align="center"
        width={MENU_WIDTH}
        label="New session"
        kind="dialog"
        onClose={onClose}
      >
        <NewSessionForm opened={request.opened} connection={connection} onClose={onClose} />
      </ShellMenu>
    );
  }

  // Cancel steps back to the list the form came from, or closes a form
  // the popover opened on.
  const cancel = () => (request.mode === 'menu' ? setMode('menu') : onClose());

  if (mode === 'rename') {
    return (
      <ShellMenu
        anchor={anchor}
        align="center"
        width={MENU_WIDTH}
        label="Rename session"
        kind="dialog"
        onClose={onClose}
      >
        <RenameSessionForm onCancel={cancel} onClose={onClose} />
      </ShellMenu>
    );
  }

  if (mode === 'edit') {
    return (
      <ShellMenu
        anchor={anchor}
        align="center"
        width={MENU_WIDTH}
        label="Edit connection"
        kind="dialog"
        onClose={onClose}
      >
        <ConnectionForm
          title="Edit connection"
          submitLabel="Save"
          initial={target}
          onCancel={cancel}
          onSubmit={(next) => {
            connection.saveTarget(next);
            onClose();
          }}
        />
      </ShellMenu>
    );
  }

  return (
    <ShellMenu
      anchor={anchor}
      align="center"
      width={MENU_WIDTH}
      label="Session"
      listed={listSessions}
      onClose={onClose}
    >
      {listSessions && (
        <>
          <p className="shell-menu-head">Sessions</p>
          <div className="shell-menu-sessions">
            {rows.map((row, i) => (
              <SessionItem
                key={row.id}
                row={row}
                rows={rows}
                place={i + 1}
                current={row.id === selected}
                onSelect={() => run(() => pick(row.id))}
                onCloseSession={() => run(() => onCloseSession?.(row.id))}
              />
            ))}
          </div>
          <ShellMenuSeparator />
        </>
      )}
      {!live && (
        <ShellMenuItem
          shortcut={shortcutLabel(APP_SHORTCUTS.connect)}
          onSelect={() => run(connection.connect)}
        >
          Connect to {worldName(target.host)}
        </ShellMenuItem>
      )}
      <ShellMenuItem onSelect={() => setMode('edit')}>Edit connection…</ShellMenuItem>
      <ShellMenuItem onSelect={() => (renameInRow ? run(renameInRow) : setMode('rename'))}>
        Rename session…
      </ShellMenuItem>
      <ShellMenuSeparator />
      <ShellMenuItem
        shortcut={shortcutLabel(APP_SHORTCUTS['session-new'])}
        onSelect={() => run(openNewSession)}
      >
        New session…
      </ShellMenuItem>
      {live && (
        <>
          <ShellMenuSeparator />
          <ShellMenuItem danger onSelect={() => run(connection.disconnect)}>
            Disconnect
          </ShellMenuItem>
        </>
      )}
    </ShellMenu>
  );
}

interface ItemProps {
  row: SessionRow;
  rows: SessionRow[];
  /** Its place in the list, from 1. */
  place: number;
  current: boolean;
  onSelect: () => void;
  onCloseSession: () => void;
}

/** One session in the popover's list, as its row in the sidebar draws
 *  it, 44 high on the menu's recipe, with its close button beside it. */
function SessionItem({ row, rows, place, current, onSelect, onCloseSession }: ItemProps) {
  const look = rowLook(useSessionRow(row.id), row, current);
  const end = current ? (
    <CheckIcon className="pane-menu-check" />
  ) : look.count > 0 ? (
    <WaitingCount count={look.count} />
  ) : (
    place <= 9 && <kbd className="shell-menu-kbd">{shortcutLabel(`Mod+${place}`)}</kbd>
  );
  return (
    <div className="shell-menu-session-slot">
      <button
        type="button"
        role="menuitem"
        className="shell-menu-session"
        aria-current={current ? 'true' : undefined}
        onClick={onSelect}
      >
        <SessionRowBody row={row} rows={rows} mark={look.mark} end={end} />
      </button>
      {/* A sibling of the row, since a button holds no button. It shows
        only under the pointer, so the arrow keys pass it by, and ⌘W
        closes the session in front from the keyboard. */}
      <button
        type="button"
        className="shell-menu-session-close"
        aria-label="Close session"
        tabIndex={-1}
        onClick={onCloseSession}
      >
        <CloseIcon />
      </button>
    </div>
  );
}
