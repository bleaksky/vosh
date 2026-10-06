import { useState } from 'react';
import APP_SHORTCUTS from '../lib/appShortcuts.json';
import type { SessionMenuRequest } from '../lib/appMenu';
import { shortcutLabel } from '../lib/shortcuts';
import { worldName } from '../lib/knownWorlds';
import type { Connection } from '../stores/session/useConnection';
import { ConnectionForm } from './ConnectionForm';
import { openNewSession } from './newSession';
import { NewSessionForm } from './NewSessionForm';
import { RenameSessionForm } from './RenameSessionForm';
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
  onClose: () => void;
}

export function SessionMenu({
  connection,
  anchor,
  request = { mode: 'menu' },
  renameInRow,
  onClose,
}: Props) {
  const [mode, setMode] = useState(request.mode);
  const { live, target } = connection;

  const run = (action: () => Promise<void> | void) => {
    onClose();
    void action();
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
    <ShellMenu anchor={anchor} align="center" width={MENU_WIDTH} label="Session" onClose={onClose}>
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
