import { useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import {
  affectsMarkerChoices,
  affectsStyleChoices,
  markerApplies,
  openPaneSubmenu,
  type MenuChoice,
  type PaneSubmenu,
  type PaneSubmenuState,
} from './affects/affectsDisplay';
import {
  CHAT_CHANNELS,
  chatChannelColor,
  chatColorChoices,
  type ChatColorChoice,
  type ChatColors,
} from './chat/chatColors';
import { setAffectsDisplay } from '../ipc/affects';
import { profilesList } from '../ipc/profiles';
import { resetChatColors, setChatColor } from '../ipc/uiConfig';
import { useAffectsDisplay } from '../stores/config/affectsDisplayStore';
import { useChatColors } from '../stores/config/chatColorsStore';
import { usePlayPalette } from '../theme/fitGameColors';
import type { XtermPalette } from '../theme/themes';
import {
  LUA_PANE,
  paneKey,
  paneRef,
  splitPane,
  type PaneLeaf,
  type PaneRef,
  type SplitDir,
} from './paneLayout';
import { openSettingsTab } from '../lib/settingsLink';
import { formatSettingsTarget } from '../lib/settingsNav';
import { MenuItem, MenuSeparator, MenuSurface, type MenuCloseReason } from '../ui/MenuSurface';
import {
  closeHere,
  paneToSplitIn,
  returnToCommandLine,
  showHereInstead,
  splitHere,
} from './paneActions';
import { submenuAt } from '../ui/menuPlacement';
import { fitsPanel } from './paneGeometry';
import { PaneTextSizeContext } from './paneTextSize';
import { CheckIcon, ChevronRightIcon } from '../ui/icons';
import { getPanelLayout } from './panelLayoutStore';
import { PANE_LABELS, paneLabel, panesToShowInstead, type PanesToShowInstead } from './paneTypes';

// The more menu on every pane header: Split right, Split
// down, Show here instead with a submenu of pane types and then, after
// a rule, the Lua panes on offer, and Close pane.
// The Affects pane adds Style and Marker, each a submenu with a check
// on the current pick, Change when affects warn, which opens Settings
// on the hours under Layout, Affects, and Edit tracked affects, which
// opens Settings on that profile's Tracked affects in Characters.
// Marker goes quiet while either chip style is chosen, since chips draw
// no marker. The hours are numbers, typed in Settings and never picked
// in a menu. The Chat pane adds Channel colors, a submenu of the eleven
// channels the game sends, each opening Default and the theme's 16 ANSI
// colors with a check on the current pick, then Reset all. A Lua pane
// adds Edit with its plugin's name, which opens that plugin under
// Scripts in Settings. A pick saves
// alone for the profile and the pane follows at once. Closing a pane
// loses nothing, so it carries no destructive color. A split the panel
// has no room for, with every pane at its minimum at your panel size,
// stays unavailable.

interface Props {
  leaf: PaneLeaf;
  /** The more button. The menu hangs 12 px under it. */
  anchor: HTMLButtonElement;
  onClose: () => void;
}

// Gap between the more button and the menu, like the session popover.
const DROP = 12;

export function PaneMenu({ leaf, anchor, onClose }: Props) {
  const [profile, setProfile] = useState<string | null>(null);
  const [subOpen, setSubOpen] = useState<PaneSubmenuState | null>(null);
  // The channel whose colors show beside Channel colors.
  const [chanOpen, setChanOpen] = useState<PaneSubmenuState<string> | null>(null);
  const rowRefs = useRef<Partial<Record<PaneSubmenu, HTMLButtonElement | null>>>({});
  const chanRefs = useRef<Partial<Record<string, HTMLButtonElement | null>>>({});
  const display = useAffectsDisplay();
  const chatColors = useChatColors();
  const palette = usePlayPalette();
  const textSize = useContext(PaneTextSizeContext);
  const menuId = `pane-menu-${leaf.id}`;
  const subId = (which: PaneSubmenu) => `${menuId}-${which}`;
  const chanId = (channel: string) => `${menuId}-colors-${channel}`;

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

  // The menu hangs from the button you pressed, its right edge on the
  // button's right edge, so it opens where you look. It moves right of
  // the button only when the window has no room to its left.
  const button = anchor.getBoundingClientRect();
  const at = {
    x: button.left,
    y: button.bottom + DROP,
    flipX: button.right,
    preferFlip: true,
    flipY: button.top - DROP,
  };

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

  const splitIn = paneToSplitIn(leaf.id);
  const showHere = panesToShowInstead(leaf, getPanelLayout()?.root ?? null);
  const area = anchor.closest('.panel-panes');
  const canSplit = (dir: SplitDir) => {
    const root = getPanelLayout()?.root;
    if (!root || splitIn === null) return false;
    if (!area) return true;
    return fitsPanel(
      splitPane(root, leaf.id, dir, splitIn),
      area.clientWidth,
      area.clientHeight,
      textSize,
    );
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
      items: () => (
        <ShowHereRows {...showHere} pick={(ref) => run(() => showHereInstead(leaf.id, ref))()} />
      ),
    },
    style: {
      label: 'Style',
      items: () => choices(affectsStyleChoices(display), (style) => pickDisplay({ style })),
    },
    marker: {
      label: 'Marker',
      items: () => choices(affectsMarkerChoices(display), (marker) => pickDisplay({ marker })),
    },
    colors: {
      label: 'Channel colors',
      items: () => (
        <ChannelColorRows
          colors={chatColors}
          palette={palette}
          open={chanOpen?.which ?? null}
          listId={chanId}
          rowRef={(channel, el) => {
            chanRefs.current[channel] = el;
          }}
          onOpen={(channel, focus) => setChanOpen((prev) => openPaneSubmenu(prev, channel, focus))}
          onLeave={(channel) =>
            setChanOpen((prev) => (prev && prev.which !== channel ? null : prev))
          }
          done={() => close('select')}
        />
      ),
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
        at={submenuAt(r, menu)}
        onClose={() => {
          // Escape or ArrowLeft: back to the row that opened it.
          setSubOpen(null);
          setChanOpen(null);
          rowRefs.current[which]?.focus();
        }}
      >
        {submenus[which].items()}
      </MenuSurface>
    );
  }

  // A channel's colors, beside Channel colors.
  let chanSub: ReactNode = null;
  const chanRow = subOpen?.which === 'colors' && chanOpen ? chanRefs.current[chanOpen.which] : null;
  if (chanOpen && chanRow) {
    const channel = chanOpen.which;
    const r = chanRow.getBoundingClientRect();
    const menu = chanRow.closest('menu')?.getBoundingClientRect() ?? r;
    // With the panel at the window's right edge, Channel colors opens to
    // the left of the pane menu, so a channel's colors keep going left
    // instead of opening back over the pane menu.
    const pane = rowRefs.current.colors?.closest('menu')?.getBoundingClientRect();
    const leftward = pane !== undefined && menu.left < pane.left;
    chanSub = (
      <MenuSurface
        key={channel}
        id={chanId(channel)}
        label={`Color for ${channel}`}
        nested
        autoFocus={chanOpen.focus}
        className="pane-menu-sub"
        at={submenuAt(r, menu, leftward)}
        onClose={() => {
          setChanOpen(null);
          chanRefs.current[channel]?.focus();
        }}
      >
        <ChannelColorItems
          channel={channel}
          colors={chatColors}
          palette={palette}
          done={() => close('select')}
        />
      </MenuSurface>
    );
  }

  const closeSub = () => {
    setSubOpen(null);
    setChanOpen(null);
  };
  /** A row that opens `which`. Pointing at it or opening it from the
   *  keyboard opens its submenu and closes any other, and the arrow keys
   *  landing on it close any other too. */
  const submenuRow = (which: PaneSubmenu, disabled: boolean) => (
    <MenuItem
      itemRef={(el) => {
        rowRefs.current[which] = el;
      }}
      disabled={disabled}
      onFocus={() => {
        setSubOpen((prev) => (prev && prev.which !== which ? null : prev));
        if (which !== 'colors') setChanOpen(null);
      }}
      submenu={{
        open: subOpen?.which === which,
        controls: subId(which),
        onOpen: (focus) => {
          setSubOpen((prev) => openPaneSubmenu(prev, which, focus));
          if (which !== 'colors') setChanOpen(null);
        },
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
        label={`${paneLabel(leaf)} options`}
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
        {submenuRow('show', showHere.builtIns.length === 0 && showHere.lua.length === 0)}
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
                  formatSettingsTarget({
                    group: 'layout',
                    section: 'affects',
                    anchor: 'affects-running-out',
                  }),
                ),
              )}
            >
              Change when affects warn…
            </MenuItem>
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
        {leaf.pane === 'chat' && (
          <>
            <MenuSeparator />
            {submenuRow('colors', false)}
          </>
        )}
        {leaf.pane === LUA_PANE && leaf.props.plugin && (
          <>
            <MenuSeparator />
            <MenuItem
              onHover={closeSub}
              onFocus={closeSub}
              onSelect={run(() =>
                openSettingsTab(
                  formatSettingsTarget({ group: 'scripts', section: leaf.props.plugin }),
                ),
              )}
            >
              {`Edit ${leaf.props.plugin} in Scripts…`}
            </MenuItem>
          </>
        )}
        <MenuSeparator />
        <MenuItem onHover={closeSub} onFocus={closeSub} onSelect={run(() => closeHere(leaf.id))}>
          Close pane
        </MenuItem>
      </MenuSurface>
      {sub}
      {chanSub}
    </>
  );
}

/** The rows of Show here instead: the built-in panes, then a rule and
 *  the Lua panes, each with the plugin that draws it. */
export function ShowHereRows({
  builtIns,
  lua,
  pick,
}: PanesToShowInstead & { pick: (ref: PaneRef) => void }) {
  return (
    <>
      {builtIns.map((t) => (
        <MenuItem key={t} onSelect={() => pick(paneRef(t))}>
          {PANE_LABELS[t]}
        </MenuItem>
      ))}
      {builtIns.length > 0 && lua.length > 0 && <MenuSeparator />}
      {lua.map((ref) => (
        <MenuItem
          key={paneKey(ref)}
          trailing={<span className="pane-menu-plugin">{ref.props.plugin}</span>}
          onSelect={() => pick(ref)}
        >
          {paneLabel(ref)}
        </MenuItem>
      ))}
    </>
  );
}

/** A dot in a color, before a row's name. */
function Swatch({ color }: { color: string }) {
  return <span className="pane-menu-swatch" style={{ background: color }} aria-hidden="true" />;
}

/** The rows of Channel colors: each channel the pane knows, with a dot
 *  in the color it shows now, each opening its color list, then Reset
 *  all, which waits until you recolor a channel. */
export function ChannelColorRows({
  colors,
  palette,
  open,
  listId,
  rowRef,
  onOpen,
  onLeave,
  done,
}: {
  colors: ChatColors;
  palette: XtermPalette;
  /** The channel whose color list shows, or null. */
  open: string | null;
  /** The id of a channel's color list. */
  listId: (channel: string) => string;
  rowRef: (channel: string, el: HTMLButtonElement | null) => void;
  onOpen: (channel: string, focus: boolean) => void;
  /** The pointer or the caret reached `channel`'s row, or Reset all for
   *  null. Any other channel's list closes. */
  onLeave: (channel: string | null) => void;
  /** Closes the menu after Reset all. */
  done: () => void;
}) {
  return (
    <>
      {CHAT_CHANNELS.map((channel) => (
        <MenuItem
          key={channel}
          itemRef={(el) => rowRef(channel, el)}
          submenu={{
            open: open === channel,
            controls: listId(channel),
            onOpen: (focus) => onOpen(channel, focus),
          }}
          onFocus={() => onLeave(channel)}
          trailing={<ChevronRightIcon className="pane-menu-chevron" />}
        >
          <Swatch color={chatChannelColor(channel, palette, colors)} />
          {channel}
        </MenuItem>
      ))}
      <MenuSeparator />
      <MenuItem
        disabled={colors.size === 0}
        onHover={() => onLeave(null)}
        onFocus={() => onLeave(null)}
        onSelect={() => {
          done();
          void resetChatColors().catch(() => undefined);
        }}
      >
        Reset all
      </MenuItem>
    </>
  );
}

/** Default, set apart, then the theme's 16 ANSI colors, each with its
 *  swatch and a check on what the channel shows now. A pick saves that
 *  channel's color alone for the profile and closes the menu. */
export function ChannelColorItems({
  channel,
  colors,
  palette,
  done,
}: {
  channel: string;
  colors: ChatColors;
  palette: XtermPalette;
  done: () => void;
}) {
  const [fallback, ...slots] = chatColorChoices(channel, colors, palette);
  const item = (choice: ChatColorChoice) => (
    <MenuItem
      key={choice.value ?? 'default'}
      onSelect={() => {
        done();
        void setChatColor(channel, choice.value).catch(() => undefined);
      }}
      trailing={choice.checked ? <CheckIcon className="pane-menu-check" /> : null}
    >
      <Swatch color={choice.swatch} />
      {choice.label}
    </MenuItem>
  );
  return (
    <>
      {fallback && item(fallback)}
      <MenuSeparator />
      {slots.map(item)}
    </>
  );
}
