import { useEffect, useRef, useState, type ReactNode } from 'react';
import {
  affectsMarkerChoices,
  affectsStyleChoices,
  markerApplies,
  openPaneSubmenu,
  type MenuChoice,
  type PaneSubmenu,
  type PaneSubmenuState,
} from '../../lib/affectsDisplay';
import { profilesList, setAffectsDisplay } from '../../lib/session';
import { useAffectsDisplay } from '../../lib/stores/affectsDisplayStore';
import { splitPane, type PaneLeaf, type SplitDir } from '../../lib/paneLayout';
import { openSettingsTab } from '../../lib/settingsLink';
import { formatSettingsTarget } from '../../lib/settingsNav';
import { MenuItem, MenuSeparator, MenuSurface, type MenuCloseReason } from './MenuSurface';
import {
  closeHere,
  paneToSplitIn,
  returnToCommandLine,
  showHereInstead,
  splitHere,
} from './paneActions';
import { fitsPanel } from './paneGeometry';
import { CheckIcon, ChevronRightIcon } from './paneIcons';
import { getPanelLayout } from './panelLayoutStore';
import { PANE_LABELS, offeredPaneTypes } from './paneTypes';

// The more menu on every pane header (SPEC 9): Split right, Split
// down, Show here instead with a submenu of pane types, and Close pane.
// The Affects pane adds Style and Marker, each a submenu with a check
// on the current pick, and Edit tracked affects, which opens Settings
// on that profile's Tracked affects in Characters. Marker goes quiet
// while Grouped chips are chosen, since chips draw no marker. Closing a
// pane loses nothing, so it carries no destructive color. A split the
// panel has no room for, with every pane at its minimum, stays
// unavailable.

interface Props {
  leaf: PaneLeaf;
  /** The more button. The menu hangs 12 px under it. */
  anchor: HTMLButtonElement;
  onClose: () => void;
}

// Gap between the more button and the menu, like the session popover.
const DROP = 12;
// The menu's left edge sits this far inside the pane.
const INSET = 8;

export function PaneMenu({ leaf, anchor, onClose }: Props) {
  const [profile, setProfile] = useState<string | null>(null);
  const [subOpen, setSubOpen] = useState<PaneSubmenuState | null>(null);
  const rowRefs = useRef<Partial<Record<PaneSubmenu, HTMLButtonElement | null>>>({});
  const display = useAffectsDisplay();
  const menuId = `pane-menu-${leaf.id}`;
  const subId = (which: PaneSubmenu) => `${menuId}-${which}`;

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

  // Like the title band's menus, closing hands the caret back to the
  // command line, unless you clicked somewhere else on purpose.
  const close = (reason: MenuCloseReason | 'select') => {
    onClose();
    if (reason !== 'outside') returnToCommandLine();
  };
  const run = (action: () => void) => () => {
    close('select');
    action();
  };

  const splitIn = paneToSplitIn();
  const others = offeredPaneTypes().filter((t) => t !== leaf.pane);
  const area = anchor.closest('.panel-panes');
  const canSplit = (dir: SplitDir) => {
    const root = getPanelLayout()?.root;
    if (!root || splitIn === null) return false;
    if (!area) return true;
    return fitsPanel(splitPane(root, leaf.id, dir, splitIn), area.clientWidth, area.clientHeight);
  };

  // Each choice list checks the current pick. A pick saves it alone,
  // so the pane and Settings follow at once.
  const choices = <T extends string>(list: MenuChoice<T>[], pick: (value: T) => void) =>
    list.map((choice) => (
      <MenuItem
        key={choice.value}
        onSelect={run(() => pick(choice.value))}
        trailing={choice.checked ? <CheckIcon className="pane-menu-check" /> : null}
      >
        {choice.label}
      </MenuItem>
    ));
  const pickDisplay = (patch: Parameters<typeof setAffectsDisplay>[0]) => {
    void setAffectsDisplay(patch).catch(() => undefined);
  };
  const submenus: Record<PaneSubmenu, { label: string; items: () => ReactNode }> = {
    show: {
      label: 'Show here instead',
      items: () =>
        others.map((t) => (
          <MenuItem key={t} onSelect={run(() => showHereInstead(leaf.id, t))}>
            {PANE_LABELS[t]}
          </MenuItem>
        )),
    },
    style: {
      label: 'Style',
      items: () => choices(affectsStyleChoices(display), (style) => pickDisplay({ style })),
    },
    marker: {
      label: 'Marker',
      items: () => choices(affectsMarkerChoices(display), (marker) => pickDisplay({ marker })),
    },
  };

  let sub: ReactNode = null;
  const row = subOpen ? rowRefs.current[subOpen.which] : null;
  if (subOpen && row) {
    const which = subOpen.which;
    const r = row.getBoundingClientRect();
    const menu = row.closest('menu')?.getBoundingClientRect() ?? r;
    sub = (
      <MenuSurface
        key={which}
        id={subId(which)}
        label={submenus[which].label}
        nested
        autoFocus={subOpen.focus}
        className="pane-menu-sub"
        at={{ x: menu.right + 4, y: r.top - 6, flipX: menu.left - 4, flipY: r.bottom + 6 }}
        onClose={() => {
          // Escape or ArrowLeft: back to the row that opened it.
          setSubOpen(null);
          rowRefs.current[which]?.focus();
        }}
      >
        {submenus[which].items()}
      </MenuSurface>
    );
  }

  const closeSub = () => setSubOpen(null);
  /** A row that opens `which`. Pointing at it or opening it from the
   *  keyboard opens its submenu and closes any other, and the arrow keys
   *  landing on it close any other too. */
  const submenuRow = (which: PaneSubmenu, disabled: boolean) => (
    <MenuItem
      itemRef={(el) => {
        rowRefs.current[which] = el;
      }}
      disabled={disabled}
      onFocus={() => setSubOpen((prev) => (prev && prev.which !== which ? null : prev))}
      submenu={{
        open: subOpen?.which === which,
        controls: subId(which),
        onOpen: (focus) => setSubOpen((prev) => openPaneSubmenu(prev, which, focus)),
      }}
      trailing={<ChevronRightIcon className="pane-menu-chevron" />}
    >
      {submenus[which].label}
    </MenuItem>
  );

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
          disabled={!canSplit('row')}
          onHover={closeSub}
          onFocus={closeSub}
          onSelect={run(() => splitHere(leaf.id, 'row'))}
        >
          Split right
        </MenuItem>
        <MenuItem
          disabled={!canSplit('column')}
          onHover={closeSub}
          onFocus={closeSub}
          onSelect={run(() => splitHere(leaf.id, 'column'))}
        >
          Split down
        </MenuItem>
        <MenuSeparator />
        {submenuRow('show', others.length === 0)}
        {leaf.pane === 'affects' && (
          <>
            <MenuSeparator />
            {submenuRow('style', false)}
            {submenuRow('marker', !markerApplies(display))}
            <MenuItem
              onHover={closeSub}
              onFocus={closeSub}
              onSelect={run(() =>
                openSettingsTab(
                  formatSettingsTarget(
                    profile
                      ? { group: 'characters', section: profile, anchor: 'tracked' }
                      : { group: 'characters', anchor: 'tracked' },
                  ),
                ),
              )}
            >
              {profile ? `Edit tracked affects for ${profile}…` : 'Edit tracked affects…'}
            </MenuItem>
          </>
        )}
        <MenuSeparator />
        <MenuItem onHover={closeSub} onFocus={closeSub} onSelect={run(() => closeHere(leaf.id))}>
          Close pane
        </MenuItem>
      </MenuSurface>
      {sub}
    </>
  );
}
