import type { KeyboardEvent } from 'react';
import { flushSync } from 'react-dom';
import { usePanelSide } from '../../stores/config/panelSideStore';
import { CloseIcon, GripIcon } from '../../ui/icons';
import type { PaneLeaf } from '../paneLayout';
import { getArranging, keyTarget, moveTo, movable, putAway } from './arrange';
import { tileDrag } from './arrangeDrag';
import { ARRANGE_KEYS_ID, focusTile } from './tileFocus';

// The tile a pane turns into while you arrange, drawn over the pane,
// which dims under it and keeps its state. Press anywhere on it and
// drag, or focus it and use the keys ARRANGE_KEYS_ID describes. Put away
// sends it to the shelf. The pane a drag holds shows as the slot it
// would land in, washed in the accent with a dashed edge.

export function ArrangeTile({
  leaf,
  label,
  slot,
}: {
  leaf: PaneLeaf;
  label: string;
  slot: boolean;
}) {
  const panelSide = usePanelSide();
  if (slot) {
    return (
      <div className="arrange-tile is-slot" aria-hidden="true">
        <span>{label}</span>
      </div>
    );
  }
  if (!movable(leaf)) {
    return (
      <div className="arrange-tile is-fixed">
        <div className="arrange-tile-head">
          <span className="arrange-tile-name">{label}</span>
        </div>
        <p className="arrange-tile-note">The writing card stays here while it is pinned.</p>
      </div>
    );
  }
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return;
    const a = getArranging();
    if (!a) return;
    // The sidebar shows its panes only while the window has room for it.
    const sidebarShows = document.querySelector('[data-tree="sidebar"]') !== null;
    const target = keyTarget(a, leaf, e.key, panelSide, sidebarShows);
    if (!target || target === 'back') return;
    e.preventDefault();
    flushSync(() => moveTo(leaf, target));
    focusTile(leaf.id);
  };
  return (
    <div
      className="arrange-tile"
      role="button"
      tabIndex={0}
      data-arrange-tile={leaf.id}
      aria-label={`${label}, drag to move`}
      aria-describedby={ARRANGE_KEYS_ID}
      onPointerDown={tileDrag(leaf)}
      onKeyDown={onKeyDown}
    >
      <div className="arrange-tile-head">
        <GripIcon className="arrange-grip" />
        <span className="arrange-tile-name">{label}</span>
        <button
          type="button"
          className="arrange-tile-away"
          aria-label={`Put ${label} away`}
          title="Put away"
          onClick={() => {
            flushSync(() => putAway(leaf));
            focusTile(leaf.id);
          }}
        >
          <CloseIcon size={12} />
        </button>
      </div>
    </div>
  );
}
