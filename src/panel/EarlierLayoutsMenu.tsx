import type { EarlierLayout } from '../ipc/panes';
import { MenuItem, MenuSurface, type MenuCloseReason, type MenuPlacement } from '../ui/MenuSurface';
import { earlierNote, untilLabel } from './earlierLayouts';

// Earlier layouts, the menu Settings > Characters > Panel layout and
// arrange mode open to put back a layout a profile had before. Each row
// says until when the layout held, newest first, and after it which step
// replaced it when that was more than a change.

interface Props {
  /** The profile's earlier layouts, newest first, read before it opens. */
  earlier: EarlierLayout[];
  at: MenuPlacement;
  /** The button that opened it, whose own press toggles it. */
  anchor: HTMLElement | null;
  onClose: (reason: MenuCloseReason) => void;
  onPick: (until: number) => void;
}

export function EarlierLayoutsMenu({ earlier, at, anchor, onClose, onPick }: Props) {
  return (
    <MenuSurface label="Earlier layouts" at={at} anchor={anchor} onClose={onClose}>
      {earlier.length === 0 ? (
        <li role="none" className="menu-note">
          Vosh keeps a layout here once you change it.
        </li>
      ) : (
        earlier.map((e) => {
          const note = earlierNote(e);
          return (
            <MenuItem
              key={e.until}
              onSelect={() => onPick(e.until)}
              trailing={note ? <span className="menu-hint">{note}</span> : null}
            >
              {untilLabel(e.until)}
            </MenuItem>
          );
        })
      )}
    </MenuSurface>
  );
}
