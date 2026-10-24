import { MenuItem, MenuSeparator, MenuSurface, type MenuPlacement } from '../../ui/MenuSurface';
import { useEffect } from 'react';

export interface HeadingMenuProps {
  group: string;
  at: MenuPlacement;
  /** The list holds unsaved changes, which a rename would write over,
   *  so Rename waits for Save or Discard. */
  dirty: boolean;
  onRename: () => void;
  onSelectAll: () => void;
  onUngroup: () => void;
  onClose: () => void;
}

/** A group heading's menu in an Automation list, from a right click,
 *  the button that shows on the heading, the menu key or Shift F10.
 *  Rename puts a field in place of the heading. */
export function HeadingMenu({
  group,
  at,
  dirty,
  onRename,
  onSelectAll,
  onUngroup,
  onClose,
}: HeadingMenuProps) {
  const pick = (run: () => void) => () => {
    onClose();
    run();
  };

  // Hand focus back to where it was, as the heading after Shift F10,
  // when the menu closes without moving it somewhere else.
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    return () => {
      const current = document.activeElement;
      if (!current || current === document.body) previous?.focus({ preventScroll: true });
    };
  }, []);

  return (
    <MenuSurface label={`${group} group options`} at={at} onClose={onClose}>
      <MenuItem disabled={dirty} onSelect={pick(onRename)}>
        Rename group…
      </MenuItem>
      {dirty && (
        <li role="none" className="menu-note">
          Save or discard your changes first.
        </li>
      )}
      <MenuItem onSelect={pick(onSelectAll)}>Select all in {group}</MenuItem>
      <MenuSeparator />
      <MenuItem onSelect={pick(onUngroup)}>Move all to no group</MenuItem>
    </MenuSurface>
  );
}
