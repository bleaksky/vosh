// Panel registry + zone model for the configurable layout.
//
// Five panels can move between four edges of the main window (plus a
// `hidden` zone). The map is constrained to vertical-leaning zones
// (left or right) because a horizontal map at full window width looks
// like a strip; everything else accepts any side.
//
// Persistence reuses the backend's `dock_layout_get`/`dock_layout_set`
// commands which round-trip a `[{ id, zone }, ...]` array. Order
// within a zone follows the array order, so the user can shuffle
// stacks just by reordering entries.

export type Zone = 'top' | 'bottom' | 'left' | 'right' | 'hidden';

export const ALL_ZONES: Zone[] = ['top', 'bottom', 'left', 'right', 'hidden'];

/** Vertical alignment within a left or right zone. Top-aligned panels
 *  stack downward from the top of the column; bottom-aligned panels
 *  stack upward from the bottom. Ignored for the top/bottom zones,
 *  which are full-width strips with no vertical anchor. */
export type Align = 'top' | 'bottom';

export const ALL_ALIGNS: Align[] = ['top', 'bottom'];

export interface PanelPlacement {
  zone: Zone;
  align: Align;
}

export type PanelId = 'map' | 'group' | 'vitals' | 'roomstrip' | 'chat';

export const ALL_PANEL_IDS: PanelId[] = ['map', 'group', 'vitals', 'roomstrip', 'chat'];

export interface PanelMeta {
  id: PanelId;
  label: string;
  description: string;
  /** Zones this panel can be assigned to. The map is left/right/hidden
   *  only because a horizontal map at full width is unusable. */
  allowedZones: Zone[];
  /** Initial zone on a brand-new install. Can be `'hidden'` for opt-in
   *  panels like chat. */
  defaultZone: Zone;
  /** Where the panel goes when restored from hidden via the topbar
   *  toggle (and no previous visible zone is remembered for the
   *  session). Must be a non-hidden zone so toggling visibility
   *  actually shows the panel somewhere. */
  homeZone: Exclude<Zone, 'hidden'>;
  defaultAlign: Align;
  /** When true the panel always takes all available height inside a
   *  left/right zone and ignores its own align value. Other panels
   *  in the same zone still respect their align (above when top,
   *  below when bottom) and stack at their natural size around the
   *  fill panel. The align dropdown is disabled in Settings for
   *  panels with this flag. */
  fillsSideZone?: boolean;
}

export const PANELS: Record<PanelId, PanelMeta> = {
  map: {
    id: 'map',
    label: 'map',
    description: 'Auto-mapped rooms from GMCP. Fills its column.',
    allowedZones: ['left', 'right', 'hidden'],
    defaultZone: 'right',
    homeZone: 'right',
    defaultAlign: 'top',
    fillsSideZone: true,
  },
  group: {
    id: 'group',
    label: 'group',
    description: 'Group member vitals from Group.Info.',
    allowedZones: ['top', 'bottom', 'left', 'right', 'hidden'],
    defaultZone: 'right',
    homeZone: 'right',
    defaultAlign: 'top',
  },
  vitals: {
    id: 'vitals',
    label: 'vitals (hp bar)',
    description: 'Your hp / mn / mv bars and tick countdown.',
    allowedZones: ['top', 'bottom', 'left', 'right', 'hidden'],
    defaultZone: 'bottom',
    homeZone: 'bottom',
    defaultAlign: 'bottom',
  },
  roomstrip: {
    id: 'roomstrip',
    label: 'room strip (area info)',
    description: 'Area name, current room, exits.',
    allowedZones: ['top', 'bottom', 'left', 'right', 'hidden'],
    defaultZone: 'top',
    homeZone: 'top',
    defaultAlign: 'top',
  },
  chat: {
    id: 'chat',
    label: 'chat',
    description: 'Channel + tell history.',
    allowedZones: ['top', 'bottom', 'left', 'right', 'hidden'],
    defaultZone: 'hidden',
    homeZone: 'bottom',
    defaultAlign: 'bottom',
  },
};

export const DEFAULT_PANEL_PLACEMENTS: Record<PanelId, PanelPlacement> = ALL_PANEL_IDS.reduce(
  (acc, id) => {
    acc[id] = { zone: PANELS[id].defaultZone, align: PANELS[id].defaultAlign };
    return acc;
  },
  {} as Record<PanelId, PanelPlacement>,
);

/** Convert the persisted dock_layout array into a `{ panelId: placement }`
 *  lookup. Missing panels fall back to defaults. Unknown ids, disallowed
 *  zones, and invalid align values are silently dropped (the default
 *  takes over). */
export function panelPlacementsFromDock(
  entries: { id: string; zone: string; align?: string }[],
): Record<PanelId, PanelPlacement> {
  const out: Record<PanelId, PanelPlacement> = { ...DEFAULT_PANEL_PLACEMENTS };
  for (const entry of entries) {
    if (!isPanelId(entry.id)) continue;
    if (!isZone(entry.zone)) continue;
    if (!PANELS[entry.id].allowedZones.includes(entry.zone)) continue;
    const align = isAlign(entry.align ?? '')
      ? (entry.align as Align)
      : PANELS[entry.id].defaultAlign;
    out[entry.id] = { zone: entry.zone, align };
  }
  return out;
}

/** Serialize a `{ panelId: placement }` lookup back into the dock_layout
 *  array. Preserves canonical panel ordering so the persisted TOML stays
 *  diff-friendly. Align is omitted when default to keep storage minimal. */
export function panelPlacementsToDock(
  placements: Record<PanelId, PanelPlacement>,
): { id: string; zone: string; align?: string }[] {
  return ALL_PANEL_IDS.map((id) => {
    const p = placements[id];
    // Align is only meaningful in vertical zones; top/bottom/hidden
    // never read it. Persisting unconditionally keeps the file stable
    // when the user moves panels across zones and back.
    const entry: { id: string; zone: string; align?: string } = { id, zone: p.zone };
    if (p.zone === 'left' || p.zone === 'right') {
      entry.align = p.align;
    }
    return entry;
  });
}

/** Group panels by their (zone, align). Result has six lists: top,
 *  bottom, and for each of left/right a top-aligned and bottom-aligned
 *  list, plus hidden. Preserves canonical panel ordering inside each
 *  list. */
export interface GroupedPanels {
  top: PanelId[];
  bottom: PanelId[];
  leftTop: PanelId[];
  leftBottom: PanelId[];
  rightTop: PanelId[];
  rightBottom: PanelId[];
  hidden: PanelId[];
}

export function groupPanels(placements: Record<PanelId, PanelPlacement>): GroupedPanels {
  const out: GroupedPanels = {
    top: [],
    bottom: [],
    leftTop: [],
    leftBottom: [],
    rightTop: [],
    rightBottom: [],
    hidden: [],
  };
  for (const id of ALL_PANEL_IDS) {
    const p = placements[id];
    switch (p.zone) {
      case 'top':
        out.top.push(id);
        break;
      case 'bottom':
        out.bottom.push(id);
        break;
      case 'left':
        (p.align === 'bottom' ? out.leftBottom : out.leftTop).push(id);
        break;
      case 'right':
        (p.align === 'bottom' ? out.rightBottom : out.rightTop).push(id);
        break;
      case 'hidden':
        out.hidden.push(id);
        break;
    }
  }
  return out;
}

function isPanelId(s: string): s is PanelId {
  return (ALL_PANEL_IDS as string[]).includes(s);
}

function isZone(s: string): s is Zone {
  return (ALL_ZONES as string[]).includes(s);
}

function isAlign(s: string): s is Align {
  return s === 'top' || s === 'bottom';
}
