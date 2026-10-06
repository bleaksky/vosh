import { forwardRef } from 'react';
import { sessionLabel } from '../lib/sessionLabel';
import { useSelected, useSessions } from '../stores/session/sessionsStore';
import type { Connection } from '../stores/session/useConnection';
import { ChevronDownIcon } from '../ui/icons';
import { useWindowTitle, windowTitle } from './windowTitle';

// The session control centered in the title band (SPEC 1 and G2): a
// status dot, the selected session in the title tone, by the name you
// gave it or your character, where it plays in the tertiary tone, and
// a chevron. No chrome at rest. It opens the session menu. The world
// takes its port when the port is not the world's own, as the session's
// row does (sessionLabel.ts), so Orla reads The Forsaken Lands 1825.

interface Props {
  connection: Connection;
  open: boolean;
  onToggle: () => void;
}

type DotKind = 'connected' | 'connecting' | 'idle' | 'error';

export const TitleButton = forwardRef<HTMLButtonElement, Props>(function TitleButton(
  { connection, open, onToggle },
  ref,
) {
  const { status, character, world } = connection;
  const rows = useSessions();
  const selected = useSelected();
  const live = status.kind === 'connected' || status.kind === 'connecting' ? status : null;
  const { who, place } = sessionLabel(
    {
      id: selected,
      name: rows.find((row) => row.id === selected)?.name ?? null,
      character,
      host: live?.host ?? null,
      port: live?.port ?? null,
    },
    rows,
  );
  const where = place ?? world;
  useWindowTitle(windowTitle(status, who, where));
  let dot: DotKind;
  let primary: string;
  let secondary: string | null = null;
  let label: string;
  switch (status.kind) {
    case 'connected':
      dot = 'connected';
      primary = who ?? where;
      secondary = who ? where : null;
      label = who ? `${who}, connected to ${where}` : `Connected to ${where}`;
      break;
    case 'connecting':
      dot = 'connecting';
      primary = 'Connecting';
      secondary = where;
      label = `Connecting to ${where}`;
      break;
    case 'error':
      dot = 'error';
      primary = 'Not connected';
      label = `Not connected. ${status.message}`;
      break;
    default:
      dot = 'idle';
      primary = 'Not connected';
      label = 'Not connected';
  }

  return (
    <button
      ref={ref}
      type="button"
      className={`shell-title-button${open ? ' is-open' : ''}`}
      aria-haspopup="menu"
      aria-expanded={open}
      aria-label={label}
      title={status.kind === 'error' ? status.message : undefined}
      onClick={onToggle}
    >
      <span className={`shell-dot is-${dot}`} aria-hidden="true" />
      <span className="shell-title-name">{primary}</span>
      {secondary && <span className="shell-title-world">{secondary}</span>}
      <span className="shell-title-chevron">
        <ChevronDownIcon size={12} />
      </span>
    </button>
  );
});
