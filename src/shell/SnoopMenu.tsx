import type { SnoopTab } from '../ipc/snoop';
import APP_SHORTCUTS from '../lib/appShortcuts.json';
import { MenuItem, MenuSeparator, MenuSurface, type MenuCloseReason } from '../ui/MenuSurface';

// The snoop split's more menu: the pane menu recipe, 232 wide, hanging
// 6 under the more button with its right edge on the button's. Stop
// snooping the player in front, or Close an ended tab, then Stop every
// snoop, which sends `snoop stop` for the game to read as stop all.
// After a rule, Find in the tab in front, Open in a window and Fold, or
// Unfold while folded. The snoop window's menu stops at Find.

/** What a row of the menu does. */
export type SnoopPick = 'stop' | 'stop-all' | 'find' | 'window' | 'fold';

interface Props {
  /** The more button the menu hangs from. */
  anchor: HTMLElement;
  /** The tab in front. */
  front: SnoopTab | null;
  /** The split's fold, which brings Open in a window and Fold. The
   *  snoop window leaves it out. */
  folded?: boolean | undefined;
  onPick: (pick: SnoopPick) => void;
  onClose: (reason: MenuCloseReason) => void;
}

/** The gap between the more button and the menu. */
const DROP = 6;

export function SnoopMenu({ anchor, front, folded, onPick, onClose }: Props) {
  const button = anchor.getBoundingClientRect();
  const at = {
    x: button.left,
    y: button.bottom + DROP,
    flipX: button.right,
    preferFlip: true,
    flipY: button.top - DROP,
  };
  // A pick closes the menu as Esc does, and what it does says where the
  // caret goes.
  const pick = (what: SnoopPick) => () => {
    onClose('escape');
    onPick(what);
  };

  return (
    <MenuSurface
      label="Snoop options"
      at={at}
      anchor={anchor}
      onClose={onClose}
      className="snoop-menu"
    >
      {front && (
        <MenuItem onSelect={pick('stop')}>
          {front.live ? `Stop snooping ${front.name}` : `Close ${front.name}`}
        </MenuItem>
      )}
      <MenuItem onSelect={pick('stop-all')}>Stop every snoop</MenuItem>
      <MenuSeparator />
      <MenuItem onSelect={pick('find')} keys={APP_SHORTCUTS.find}>
        Find
      </MenuItem>
      {folded !== undefined && (
        <>
          <MenuItem onSelect={pick('window')}>Open in a window</MenuItem>
          <MenuItem onSelect={pick('fold')}>{folded ? 'Unfold' : 'Fold'}</MenuItem>
        </>
      )}
    </MenuSurface>
  );
}
