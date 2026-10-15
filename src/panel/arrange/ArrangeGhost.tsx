import { GripIcon } from '../../ui/icons';
import { paneLabel } from '../paneTypes';
import { useArranging } from './arrange';

// The tile under the pointer while a drag runs in arrange mode, with
// the pane's name, kept inside the window. Nothing while no drag runs.

/** The ghost's size in frame.css. */
const GHOST_W = 180;
const GHOST_H = 32;

export function ArrangeGhost() {
  const drag = useArranging()?.drag;
  if (!drag?.measured) return null;
  const left = Math.max(0, Math.min(drag.x - 18, window.innerWidth - GHOST_W));
  const top = Math.max(0, Math.min(drag.y - 16, window.innerHeight - GHOST_H));
  return (
    <div className="arrange-ghost" aria-hidden="true" style={{ left, top }}>
      <GripIcon className="arrange-grip" />
      <span className="arrange-tile-name">{paneLabel(drag.leaf)}</span>
    </div>
  );
}
