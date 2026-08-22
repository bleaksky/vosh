// Panel registry + zone model for the configurable layout.
//
// Six panels can move between four edges of the main window (plus a
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

/** True when the zone is one of the regular layout zones (renders the
 *  panel as a full panel in its own slot). */
export function isLayoutZone(z: Zone): boolean {
  return z === 'top' || z === 'bottom' || z === 'left' || z === 'right';
}

/** Display label for the zone dropdown. */
export function zoneLabel(z: Zone): string {
  return z;
}

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

export type PanelId =
  | 'map'
  | 'group'
  | 'vitals'
  | 'roomstrip'
  | 'chat'
  | 'affects'
  | 'combat'
  | 'imm';

export const ALL_PANEL_IDS: PanelId[] = [
  'map',
  'group',
  'vitals',
  'roomstrip',
  'chat',
  'affects',
  'combat',
  'imm',
];

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
    description: 'Your hp / mn / mv bars.',
    allowedZones: ['top', 'bottom', 'left', 'right', 'hidden'],
    // Ember rail order: map, vitals, affects, group down the right
    // side, per the approved canvas.
    defaultZone: 'right',
    homeZone: 'right',
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
  affects: {
    id: 'affects',
    label: 'affects',
    description: 'Tracked affects with remaining duration.',
    allowedZones: ['top', 'bottom', 'left', 'right', 'hidden'],
    defaultZone: 'right',
    homeZone: 'right',
    defaultAlign: 'bottom',
  },
  imm: {
    id: 'imm',
    label: 'imm (staff queues)',
    description:
      'Staff duty board fed by the Imm.Queues push. The server lights it for immortals and mortals never receive the feed.',
    allowedZones: ['top', 'bottom', 'left', 'right', 'hidden'],
    defaultZone: 'hidden',
    homeZone: 'right',
    defaultAlign: 'top',
  },
  combat: {
    id: 'combat',
    label: 'combat',
    description:
      'Current target name and hp bar. Hidden = inline inside the vitals bar (legacy). Bottom = chip variant flush with whatever sits below. Other zones = standalone pane.',
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

/** Layout state derived from the persisted dock_layout array. `order`
 *  is the canonical render order — the position of a panel within its
 *  zone follows this list, so the user can reorder by shuffling entries
 *  in the dock layout. `placements` is the lookup by id. */
export interface PanelLayout {
  placements: Record<PanelId, PanelPlacement>;
  order: PanelId[];
}

export const DEFAULT_PANEL_LAYOUT: PanelLayout = {
  placements: DEFAULT_PANEL_PLACEMENTS,
  order: [...ALL_PANEL_IDS],
};

/** Convert the persisted dock_layout array into a {@link PanelLayout}.
 *  Missing panels fall back to defaults at the end of `order`. Unknown
 *  ids and disallowed zones / aligns are silently dropped. */
export function panelLayoutFromDock(
  entries: { id: string; zone: string; align?: string }[],
): PanelLayout {
  const placements: Record<PanelId, PanelPlacement> = { ...DEFAULT_PANEL_PLACEMENTS };
  const order: PanelId[] = [];
  const seen = new Set<PanelId>();
  for (const entry of entries) {
    if (!isPanelId(entry.id)) continue;
    if (!isZone(entry.zone)) continue;
    if (!PANELS[entry.id].allowedZones.includes(entry.zone)) continue;
    if (seen.has(entry.id)) continue;
    const align = isAlign(entry.align ?? '')
      ? (entry.align as Align)
      : PANELS[entry.id].defaultAlign;
    placements[entry.id] = { zone: entry.zone, align };
    order.push(entry.id);
    seen.add(entry.id);
  }
  // Newly added panel ids that have never been persisted go to the end
  // so they show up at the bottom of their zone with default placement.
  for (const id of ALL_PANEL_IDS) {
    if (!seen.has(id)) order.push(id);
  }
  return { placements, order };
}

/** Serialize a {@link PanelLayout} back into the dock_layout array.
 *  Honors the layout's order so user-driven reordering survives a
 *  round-trip. Align is omitted for top/bottom/hidden zones where it
 *  is meaningless. */
export function panelLayoutToDock(
  layout: PanelLayout,
): { id: string; zone: string; align?: string }[] {
  return layout.order.map((id) => {
    const p = layout.placements[id];
    const entry: { id: string; zone: string; align?: string } = { id, zone: p.zone };
    if (p.zone === 'left' || p.zone === 'right') {
      entry.align = p.align;
    }
    return entry;
  });
}

/** Group panels by their (zone, align). Result has six lists: top,
 *  bottom, and for each of left/right a top-aligned and bottom-aligned
 *  list, plus hidden. Iteration order within each list comes from
 *  `layout.order`, so a user who reorders the bottom zone sees their
 *  preferred sequence. */
export interface GroupedPanels {
  top: PanelId[];
  bottom: PanelId[];
  leftTop: PanelId[];
  leftBottom: PanelId[];
  rightTop: PanelId[];
  rightBottom: PanelId[];
  hidden: PanelId[];
}

export function groupPanels(layout: PanelLayout): GroupedPanels {
  const out: GroupedPanels = {
    top: [],
    bottom: [],
    leftTop: [],
    leftBottom: [],
    rightTop: [],
    rightBottom: [],
    hidden: [],
  };
  for (const id of layout.order) {
    const p = layout.placements[id];
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
      default:
        // Inline-host zones do not show up as standalone panels in any
        // layout slot. Their host renders them inline.
        break;
    }
  }
  return out;
}

/** Return the list of panel ids that share a zone (and, for side zones,
 *  align) with the given panel. The order follows `layout.order`. */
function siblingsInGroup(layout: PanelLayout, id: PanelId): PanelId[] {
  const p = layout.placements[id];
  const grouped = groupPanels(layout);
  switch (p.zone) {
    case 'top':
      return grouped.top;
    case 'bottom':
      return grouped.bottom;
    case 'left':
      return p.align === 'bottom' ? grouped.leftBottom : grouped.leftTop;
    case 'right':
      return p.align === 'bottom' ? grouped.rightBottom : grouped.rightTop;
    case 'hidden':
      return grouped.hidden;
    default:
      // Inline-host zones have no sibling group — the panel renders
      // inside its host, not in a layout slot.
      return [];
  }
}

/** Swap `id` with the previous or next panel in the same zone-and-align
 *  group. Returns the layout unchanged if there is no sibling in the
 *  given direction. */
export function movePanelInZone(
  layout: PanelLayout,
  id: PanelId,
  direction: 'up' | 'down',
): PanelLayout {
  const siblings = siblingsInGroup(layout, id);
  const groupIdx = siblings.indexOf(id);
  if (groupIdx < 0) return layout;
  const neighborGroupIdx = direction === 'up' ? groupIdx - 1 : groupIdx + 1;
  if (neighborGroupIdx < 0 || neighborGroupIdx >= siblings.length) return layout;
  const neighborId = siblings[neighborGroupIdx];
  const nextOrder = [...layout.order];
  const a = nextOrder.indexOf(id);
  const b = nextOrder.indexOf(neighborId);
  if (a < 0 || b < 0) return layout;
  [nextOrder[a], nextOrder[b]] = [nextOrder[b], nextOrder[a]];
  return { placements: layout.placements, order: nextOrder };
}

/** True when `id` has a sibling above it in the same group. */
export function canMovePanelUp(layout: PanelLayout, id: PanelId): boolean {
  const siblings = siblingsInGroup(layout, id);
  return siblings.indexOf(id) > 0;
}

/** True when `id` has a sibling below it in the same group. */
export function canMovePanelDown(layout: PanelLayout, id: PanelId): boolean {
  const siblings = siblingsInGroup(layout, id);
  const idx = siblings.indexOf(id);
  return idx >= 0 && idx < siblings.length - 1;
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
