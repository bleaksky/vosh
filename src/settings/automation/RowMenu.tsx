import { useEffect, useId, useRef, useState } from 'react';
import { countPhrase, type KindNoun } from '../../automation/automationDraft';
import { CheckIcon, ChevronRightIcon } from '../../ui';
import { MenuItem, MenuSeparator, MenuSurface, type MenuPlacement } from '../../ui/MenuSurface';
import { submenuAt } from '../../ui/menuPlacement';

export interface RowMenuProps {
  noun: KindNoun;
  /** Where the pointer was, or the row's corner from the keyboard. */
  at: MenuPlacement;
  /** How many rows the menu acts on: the checked rows when you opened it
   *  on one of them, else the row alone. */
  count: number;
  /** Every group in the four lists. */
  groups: readonly string[];
  /** The group the rows share, blank for none, or null when they differ. */
  shared: string | null;
  /** The group of the row you opened the menu on, blank for none. */
  rowGroup: string;
  canTurnOn: boolean;
  canTurnOff: boolean;
  canDuplicate: boolean;
  /** How many of the rows you can delete. */
  deletable: number;
  /** Picking is on already, so Select several has nothing to start. */
  picking: boolean;
  onMove: (group: string) => void;
  /** Put you in the Group field, to type a new group. */
  onNewGroup: () => void;
  onTurn: (on: boolean) => void;
  onDuplicate: () => void;
  onDelete: () => void;
  /** Check every row of the row's group. */
  onSelectGroup: () => void;
  onSelectSeveral: () => void;
  onClose: () => void;
}

/** A row's menu in an Automation list, from a right click, the menu key
 *  or Shift F10. Move to group lists your groups in a submenu, with No
 *  group and New group after them. What it does goes into the draft, as
 *  the card's buttons do. */
export function RowMenu(props: RowMenuProps) {
  const { noun, at, count, groups, shared, rowGroup, deletable, picking, onClose } = props;
  const menuId = useId();
  const subId = useId();
  const moveRef = useRef<HTMLButtonElement | null>(null);
  const [sub, setSub] = useState<{ focus: boolean } | null>(null);

  // Hand focus back to where it was, as the row after Shift F10, when
  // the menu closes without moving it somewhere else, as New group…
  // and Delete do.
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    return () => {
      const current = document.activeElement;
      if (!current || current === document.body) previous?.focus({ preventScroll: true });
    };
  }, []);
  const pick = (run: () => void) => () => {
    onClose();
    run();
  };
  const many = count > 1 ? ` ${countPhrase(count, noun)}` : '';

  let submenu = null;
  const menuEl = document.getElementById(menuId);
  if (sub && moveRef.current && menuEl) {
    submenu = (
      <MenuSurface
        id={subId}
        label="Move to group"
        nested
        autoFocus={sub.focus}
        className="menu-sub"
        at={submenuAt(moveRef.current.getBoundingClientRect(), menuEl.getBoundingClientRect())}
        onClose={() => setSub(null)}
      >
        {groups.map((g) => (
          <MenuItem
            key={g}
            onSelect={pick(() => props.onMove(g))}
            trailing={g === shared ? <CheckIcon size={12} className="menu-check" /> : undefined}
          >
            {g}
          </MenuItem>
        ))}
        {groups.length > 0 && <MenuSeparator />}
        <MenuItem
          onSelect={pick(() => props.onMove(''))}
          trailing={shared === '' ? <CheckIcon size={12} className="menu-check" /> : undefined}
        >
          No group
        </MenuItem>
        <MenuItem onSelect={pick(props.onNewGroup)}>New group…</MenuItem>
      </MenuSurface>
    );
  }

  return (
    <>
      <MenuSurface id={menuId} label={`${noun.one} options`} at={at} onClose={onClose}>
        <MenuItem
          itemRef={(el) => {
            moveRef.current = el;
          }}
          submenu={{
            open: sub !== null,
            controls: subId,
            onOpen: (focus) => setSub((prev) => (prev && !focus ? prev : { focus })),
            onClose: () => setSub(null),
          }}
          trailing={<ChevronRightIcon size={12} className="menu-chevron" />}
        >
          Move to group
        </MenuItem>
        {props.canTurnOn && (
          <MenuItem onHover={() => setSub(null)} onSelect={pick(() => props.onTurn(true))}>
            Turn on{many}
          </MenuItem>
        )}
        {props.canTurnOff && (
          <MenuItem onHover={() => setSub(null)} onSelect={pick(() => props.onTurn(false))}>
            Turn off{many}
          </MenuItem>
        )}
        {props.canDuplicate && (
          <MenuItem onHover={() => setSub(null)} onSelect={pick(props.onDuplicate)}>
            Duplicate{many}
          </MenuItem>
        )}
        <MenuSeparator />
        {rowGroup !== '' && (
          <MenuItem onHover={() => setSub(null)} onSelect={pick(props.onSelectGroup)}>
            Select all in {rowGroup}
          </MenuItem>
        )}
        {!picking && (
          <MenuItem onHover={() => setSub(null)} onSelect={pick(props.onSelectSeveral)}>
            Select several
          </MenuItem>
        )}
        {deletable > 0 && (
          <>
            <MenuSeparator />
            <MenuItem danger onHover={() => setSub(null)} onSelect={pick(props.onDelete)}>
              Delete{deletable > 1 ? ` ${countPhrase(deletable, noun)}` : ''}
            </MenuItem>
          </>
        )}
      </MenuSurface>
      {submenu}
    </>
  );
}
