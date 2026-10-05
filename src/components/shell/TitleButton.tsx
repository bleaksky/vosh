import { forwardRef } from 'react';
import type { Connection } from '../../stores/session/useConnection';
import { ChevronDownIcon } from './icons';
import { useWindowTitle, windowTitle } from './windowTitle';

// The session control centered in the title band (SPEC 1 and G2): a
// status dot, your character in the title tone, the world in the
// tertiary tone, and a chevron. No chrome at rest. It opens the
// session menu.

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
  useWindowTitle(windowTitle(status, character, world));
  let dot: DotKind;
  let primary: string;
  let secondary: string | null = null;
  let label: string;
  switch (status.kind) {
    case 'connected':
      dot = 'connected';
      primary = character ?? world;
      secondary = character ? world : null;
      label = character ? `${character}, connected to ${world}` : `Connected to ${world}`;
      break;
    case 'connecting':
      dot = 'connecting';
      primary = 'Connecting';
      secondary = world;
      label = `Connecting to ${world}`;
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
        <ChevronDownIcon />
      </span>
    </button>
  );
});
