import { useRef, useState, type ReactNode } from 'react';
import { toggleFold, usePaneLeaf } from './paneActions';
import { ChevronDownIcon, ChevronRightIcon, MoreIcon } from '../ui/icons';
import { PaneMenu } from './PaneMenu';
import { paneLabel } from './paneTypes';

// The 28 px header every pane opens with: the caps label at
// x 18, an optional meta 8 px after it, and the more button 8 px from
// the pane's right edge. No fill and no line under it. A press on it
// folds the pane down to the header, or opens it again, and a chevron
// in the gutter before the label says which, on hover and while it is
// folded. A pane moves in arrange mode (arrange/arrange.ts), never from
// its header.

interface Props {
  /** Short state beside the label, like the area name or a count. */
  meta?: ReactNode;
  /** The label in place of the one the leaf names, as a Lua pane shows
   *  the title its plugin draws now. */
  title?: string;
}

// Controls in a header keep their own presses.
const OWN_PRESS = 'button, select, input, textarea, a, [role="menu"]';

export function PaneHeader({ meta, title }: Props) {
  const leaf = usePaneLeaf();
  const moreRef = useRef<HTMLButtonElement | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  if (!leaf) return null;
  const label = title ?? paneLabel(leaf);
  const anchor = moreRef.current;
  const folded = leaf.folded === true;

  return (
    <div
      className={`pane-header${folded ? ' is-folded' : ''}`}
      onClick={(e) => {
        if ((e.target as Element).closest(OWN_PRESS)) return;
        toggleFold(leaf.id);
      }}
    >
      <button
        type="button"
        className="pane-fold"
        aria-label={folded ? `Unfold ${label}` : `Fold ${label}`}
        aria-expanded={!folded}
        onClick={() => toggleFold(leaf.id)}
      >
        {folded ? <ChevronRightIcon size={12} /> : <ChevronDownIcon size={12} />}
      </button>
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
