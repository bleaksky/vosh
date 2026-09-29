import { useEffect, useRef, useState } from 'react';
import { profilesList } from '../../lib/session';
import type { PaneLeaf } from '../../lib/paneLayout';
import { MenuItem, MenuSeparator, MenuSurface, type MenuCloseReason } from './MenuSurface';
import {
  closeHere,
  openSettingsTab,
  paneToSplitIn,
  showHereInstead,
  splitHere,
} from './paneActions';
import { ChevronRightIcon } from './paneIcons';
import { PANE_LABELS, offeredPaneTypes } from './paneTypes';

// The more menu on every pane header (SPEC 9): Split right, Split
// down, Show here instead with a submenu of pane types, Edit tracked
// affects on the Affects pane, and Close pane. Closing a pane loses
// nothing, so it carries no destructive color.

interface Props {
  leaf: PaneLeaf;
  /** The more button. The menu hangs 12 px under it and returns focus
   *  to it on Escape. */
  anchor: HTMLButtonElement;
  onClose: () => void;
}

// Gap between the more button and the menu, like the session popover.
const DROP = 12;
// The menu's left edge sits this far inside the pane.
const INSET = 8;

export function PaneMenu({ leaf, anchor, onClose }: Props) {
  const [profile, setProfile] = useState<string | null>(null);
  const [subOpen, setSubOpen] = useState<{ focus: boolean } | null>(null);
  const showRowRef = useRef<HTMLButtonElement | null>(null);
  const menuId = `pane-menu-${leaf.id}`;
  const subId = `${menuId}-show`;

  useEffect(() => {
    if (leaf.pane !== 'affects') return;
    let live = true;
    profilesList()
      .then((list) => {
        if (live) setProfile(list.active);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [leaf.pane]);

  const button = anchor.getBoundingClientRect();
  const pane = anchor.closest('section')?.getBoundingClientRect() ?? button;
  const at = { x: pane.left + INSET, y: button.bottom + DROP, flipY: button.top - DROP };

  const close = (reason: MenuCloseReason | 'select') => {
    onClose();
    if (reason === 'escape') anchor.focus();
  };
  const run = (action: () => void) => () => {
    close('select');
    action();
  };

  const splitIn = paneToSplitIn();
  const others = offeredPaneTypes().filter((t) => t !== leaf.pane);

  let sub: React.ReactNode = null;
  const row = showRowRef.current;
  if (subOpen && row) {
    const r = row.getBoundingClientRect();
    const menu = row.closest('menu')?.getBoundingClientRect() ?? r;
    sub = (
      <MenuSurface
        id={subId}
        label="Show here instead"
        nested
        autoFocus={subOpen.focus}
        className="pane-menu-sub"
        at={{ x: menu.right + 4, y: r.top - 6, flipX: menu.left - 4, flipY: r.bottom + 6 }}
        onClose={() => {
          // Escape or ArrowLeft: back to the row that opened it.
          setSubOpen(null);
          showRowRef.current?.focus();
        }}
      >
        {others.map((t) => (
          <MenuItem key={t} onSelect={run(() => showHereInstead(leaf.id, t))}>
            {PANE_LABELS[t]}
          </MenuItem>
        ))}
      </MenuSurface>
    );
  }

  const closeSub = () => setSubOpen(null);

  return (
    <>
      <MenuSurface
        id={menuId}
        label={`${PANE_LABELS[leaf.pane]} options`}
        anchor={anchor}
        at={at}
        onClose={close}
      >
        <MenuItem
          disabled={splitIn === null}
          onHover={closeSub}
          onSelect={run(() => splitHere(leaf.id, 'row'))}
        >
          Split right
        </MenuItem>
        <MenuItem
          disabled={splitIn === null}
          onHover={closeSub}
          onSelect={run(() => splitHere(leaf.id, 'column'))}
        >
          Split down
        </MenuItem>
        <MenuSeparator />
        <MenuItem
          itemRef={(el) => {
            showRowRef.current = el;
          }}
          disabled={others.length === 0}
          submenu={{
            open: subOpen !== null,
            controls: subId,
            onOpen: (focus) => setSubOpen((prev) => (prev && !focus ? prev : { focus })),
          }}
          trailing={<ChevronRightIcon className="pane-menu-chevron" />}
        >
          Show here instead
        </MenuItem>
        {leaf.pane === 'affects' && (
          <>
            <MenuSeparator />
            <MenuItem onHover={closeSub} onSelect={run(() => openSettingsTab('profiles'))}>
              {profile ? `Edit tracked affects for ${profile}…` : 'Edit tracked affects…'}
            </MenuItem>
          </>
        )}
        <MenuSeparator />
        <MenuItem onHover={closeSub} onSelect={run(() => closeHere(leaf.id))}>
          Close pane
        </MenuItem>
      </MenuSurface>
      {sub}
    </>
  );
}
