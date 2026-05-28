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

export type PanelId = 'map' | 'group' | 'vitals' | 'roomstrip' | 'chat';

export const ALL_PANEL_IDS: PanelId[] = ['map', 'group', 'vitals', 'roomstrip', 'chat'];

export interface PanelMeta {
  id: PanelId;
  label: string;
  description: string;
  /** Zones this panel can be assigned to. The map is left/right/hidden
   *  only because a horizontal map at full width is unusable. */
  allowedZones: Zone[];
  defaultZone: Zone;
}

export const PANELS: Record<PanelId, PanelMeta> = {
  map: {
    id: 'map',
    label: 'map',
    description: 'Auto-mapped rooms from GMCP. Vertical pane.',
    allowedZones: ['left', 'right', 'hidden'],
    defaultZone: 'right',
  },
  group: {
    id: 'group',
    label: 'group',
    description: 'Group member vitals from Group.Info.',
    allowedZones: ['top', 'bottom', 'left', 'right', 'hidden'],
    defaultZone: 'right',
  },
  vitals: {
    id: 'vitals',
    label: 'vitals (hp bar)',
    description: 'Your hp / mn / mv bars and tick countdown.',
    allowedZones: ['top', 'bottom', 'left', 'right', 'hidden'],
    defaultZone: 'bottom',
  },
  roomstrip: {
    id: 'roomstrip',
    label: 'room strip (area info)',
    description: 'Area name, current room, exits.',
    allowedZones: ['top', 'bottom', 'left', 'right', 'hidden'],
    defaultZone: 'top',
  },
  chat: {
    id: 'chat',
    label: 'chat',
    description: 'Channel + tell history.',
    allowedZones: ['top', 'bottom', 'left', 'right', 'hidden'],
    defaultZone: 'hidden',
  },
};

export const DEFAULT_PANEL_ZONES: Record<PanelId, Zone> = ALL_PANEL_IDS.reduce(
  (acc, id) => {
    acc[id] = PANELS[id].defaultZone;
    return acc;
  },
  {} as Record<PanelId, Zone>,
);

/** Convert the persisted dock_layout array into a `{ panelId: zone }`
 *  lookup. Missing panels fall back to defaults. Unknown ids and
 *  disallowed zones are silently dropped. */
export function panelZonesFromDock(entries: { id: string; zone: string }[]): Record<PanelId, Zone> {
  const out: Record<PanelId, Zone> = { ...DEFAULT_PANEL_ZONES };
  for (const entry of entries) {
    if (!isPanelId(entry.id)) continue;
    if (!isZone(entry.zone)) continue;
    if (!PANELS[entry.id].allowedZones.includes(entry.zone)) continue;
    out[entry.id] = entry.zone;
  }
  return out;
}

/** Serialize a `{ panelId: zone }` lookup back into the dock_layout
 *  array. Preserves the canonical panel ordering so the persisted
 *  TOML stays diff-friendly. */
export function panelZonesToDock(zones: Record<PanelId, Zone>): { id: string; zone: string }[] {
  return ALL_PANEL_IDS.map((id) => ({ id, zone: zones[id] }));
}

/** Group panels by zone, preserving canonical ordering inside each zone. */
export function groupPanelsByZone(zones: Record<PanelId, Zone>): Record<Zone, PanelId[]> {
  const out: Record<Zone, PanelId[]> = {
    top: [],
    bottom: [],
    left: [],
    right: [],
    hidden: [],
  };
  for (const id of ALL_PANEL_IDS) {
    out[zones[id]].push(id);
  }
  return out;
}

function isPanelId(s: string): s is PanelId {
  return (ALL_PANEL_IDS as string[]).includes(s);
}

function isZone(s: string): s is Zone {
  return (ALL_ZONES as string[]).includes(s);
}
