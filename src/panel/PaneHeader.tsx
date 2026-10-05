import { useRef, useState, type ReactNode } from 'react';
import { usePaneLeaf } from './paneActions';
import { MoreIcon } from '../components/panel/paneIcons';
import { PaneMenu } from './PaneMenu';
import { PANE_LABELS } from './paneTypes';

// The 28 px header every pane opens with (SPEC 9): the caps label at
// x 18, an optional meta 8 px after it, and the more button 8 px from
// the pane's right edge. No fill and no line under it.

interface Props {
  /** Short state beside the label, like the area name or a count. */
  meta?: ReactNode;
}

export function PaneHeader({ meta }: Props) {
  const leaf = usePaneLeaf();
  const moreRef = useRef<HTMLButtonElement | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  if (!leaf) return null;
  const label = PANE_LABELS[leaf.pane];
  const anchor = moreRef.current;

  return (
    <div className="pane-header">
      <h2 className="pane-label">{label}</h2>
      {meta}
      <button
        ref={moreRef}
        type="button"
        className="pane-more"
        aria-label={`${label} options`}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        aria-controls={menuOpen ? `pane-menu-${leaf.id}` : undefined}
        onClick={() => setMenuOpen((open) => !open)}
      >
        <MoreIcon />
      </button>
      {menuOpen && anchor && (
        <PaneMenu leaf={leaf} anchor={anchor} onClose={() => setMenuOpen(false)} />
      )}
    </div>
  );
}

/** Header meta text. `tone` picks the color: secondary by default,
 *  tertiary for quiet counts, danger when something needs you. */
export function PaneMeta({
  children,
  tone,
}: {
  children: ReactNode;
  tone?: 'quiet' | 'danger' | 'warn';
}) {
  return <span className={`pane-meta${tone ? ` pane-meta-${tone}` : ''}`}>{children}</span>;
}
