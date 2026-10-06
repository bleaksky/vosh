import { createSessionStore } from '../sessionStore';
import { asNumber, asText } from '../store';

// The room you stand in, for the rows under the Map pane. Room.Info
// gives the name, vnum, area name, sector, climate region and exits.
// Room.Chars lists the people you can see. Map.Tiles carries an `areas`
// dict keyed by area VNUM plus the grid cell you stand on (`h`), whose
// `ar` names your area vnum. Room.Info names the area but sends no
// vnum, so resolveArea finds it in the dict by your cell's vnum or by
// the area name.
//
// Room.* arrives only when you move or look. The store keeps the last
// room across a disconnect, the way the Map pane keeps the last map.

/** Aabahran's sector_type names, in index order. Room.Info sends the
 *  index as `sector`. Older builds send only the `terrain` name. */
const SECTOR_NAMES = [
  'inside',
  'city',
  'field',
  'forest',
  'hills',
  'mountain',
  'water_swim',
  'water_noswim',
  'swamp',
  'air',
  'desert',
  'lava',
  'snow',
];

/** Aabahran's climate regions, in index order (tables.c
 *  region_table). The new build's Room.Info sends the index as
 *  `region` and the name as `climate`. The prompt's `%G` prints the
 *  same names. */
const REGION_NAMES = [
  'Temperate',
  'Coastal North',
  'Coastal South',
  'Desert',
  'Tundra',
  'Mountain North',
  'Mountain South',
  'Mountain East',
];

const EXIT_ORDER = ['north', 'east', 'south', 'west', 'up', 'down'];

export interface RoomInfo {
  name: string;
  vnum: number | null;
  /** Area name as Room.Info sends it. */
  area: string | null;
  /** Area vnum, resolved against Map.Tiles. */
  areaVnum: number | null;
  /** The area's map tint from Map.Tiles, a hex color. */
  areaColor: string | null;
  /** Direction names with a way through, compass order first. */
  exits: string[];
  /** Sector index 0..12, the same index Map.Tiles cells use. */
  sector: number | null;
  terrain: string | null;
  /** The climate region the room's area lies in, by its name, like
   *  `Coastal North`. Null from a build that sends no region. */
  region: string | null;
}

export interface RoomPerson {
  name: string;
  npc: boolean;
}

export interface RoomState {
  info: RoomInfo | null;
  /** Room.Chars in server order, so the client target's 1 based
   *  room_idx still points at the right row. */
  people: RoomPerson[];
}

export interface MapArea {
  name: string | null;
  color: string | null;
}

/** Room.Info without the Map.Tiles area fields. */
export type RoomInfoBase = Omit<RoomInfo, 'areaVnum' | 'areaColor'>;

/** Parse Room.Info. null without a room name. */
export function parseRoomInfo(data: unknown): RoomInfoBase | null {
  if (!data || typeof data !== 'object') return null;
  const d = data as Record<string, unknown>;
  const name = asText(d.name);
  if (!name) return null;
  const terrain = asText(d.terrain);
  const sectorRaw = asNumber(d.sector);
  let sector: number | null =
    sectorRaw !== null && sectorRaw >= 0 && sectorRaw < SECTOR_NAMES.length
      ? Math.floor(sectorRaw)
      : null;
  if (sector === null && terrain) {
    const idx = SECTOR_NAMES.indexOf(terrain.toLowerCase());
    if (idx >= 0) sector = idx;
  }
  return {
    name,
    vnum: asNumber(d.num ?? d.vnum),
    area: asText(d.area),
    exits: parseExits(d.exits),
    sector,
    terrain,
    region: parseRegion(d.climate, d.region),
  };
}

/** The region's name: `climate` as sent, or else the name of the
 *  `region` index. Null when neither names one. */
export function parseRegion(climate: unknown, region: unknown): string | null {
  const name = asText(climate);
  if (name) return name;
  const idx = asNumber(region);
  if (idx === null || !Number.isInteger(idx)) return null;
  return REGION_NAMES[idx] ?? null;
}

/** Exits with a destination, compass order first, then any others in
 *  the order the server sent them. */
export function parseExits(raw: unknown): string[] {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return [];
  const exits = raw as Record<string, unknown>;
  const open = Object.keys(exits).filter((dir) => {
    const to = exits[dir];
    return to !== undefined && to !== null && to !== 0 && to !== '0' && to !== '';
  });
  const known = EXIT_ORDER.filter((dir) => open.includes(dir));
  const rest = open.filter((dir) => !EXIT_ORDER.includes(dir));
  return [...known, ...rest];
}

/** Parse Room.Chars. Aabahran sends `[{name, npc}]`. */
export function parsePeople(data: unknown): RoomPerson[] {
  if (!Array.isArray(data)) return [];
  const out: RoomPerson[] = [];
  for (const raw of data) {
    if (!raw || typeof raw !== 'object') continue;
    const r = raw as Record<string, unknown>;
    const name = asText(r.name);
    if (!name) continue;
    out.push({ name, npc: r.npc === true || r.npc === 1 || r.npc === '1' || r.npc === 'true' });
  }
  return out;
}

export interface PeopleGroup {
  name: string;
  count: number;
  npc: boolean;
  /** 1 based positions in Room.Chars, for matching the client target's
   *  room_idx after duplicates fold. */
  positions: number[];
}

/** Fold people who share a name into one row with a count, in order of
 *  first appearance. */
export function groupPeople(people: readonly RoomPerson[]): PeopleGroup[] {
  const byName = new Map<string, PeopleGroup>();
  people.forEach((person, idx) => {
    const group = byName.get(person.name);
    if (group) {
      group.count += 1;
      group.positions.push(idx + 1);
    } else {
      byName.set(person.name, {
        name: person.name,
        count: 1,
        npc: person.npc,
        positions: [idx + 1],
      });
    }
  });
  return [...byName.values()];
}

export interface MapTilesAreas {
  areas: Record<string, MapArea>;
  /** Area vnum of the cell you stand on, when the grid marks one. */
  here: number | null;
}

/** Pull the area dict and your cell's area vnum out of Map.Tiles. */
export function parseMapAreas(data: unknown): MapTilesAreas | null {
  if (!data || typeof data !== 'object') return null;
  const d = data as Record<string, unknown>;
  const areas: Record<string, MapArea> = {};
  if (d.areas && typeof d.areas === 'object') {
    for (const [vnum, raw] of Object.entries(d.areas as Record<string, unknown>)) {
      if (!raw || typeof raw !== 'object') continue;
      const a = raw as Record<string, unknown>;
      const color = asText(a.color);
      areas[vnum] = {
        name: asText(a.name),
        color: color && /^#[0-9a-f]{6}$/i.test(color) ? color : null,
      };
    }
  }
  let here: number | null = null;
  if (Array.isArray(d.g)) {
    scan: for (const row of d.g) {
      if (!Array.isArray(row)) continue;
      for (const cell of row) {
        if (!cell || typeof cell !== 'object') continue;
        const c = cell as Record<string, unknown>;
        if (c.h === 1 || c.h === '1' || c.h === true) {
          here = asNumber(c.ar);
          break scan;
        }
      }
    }
  }
  return { areas, here };
}

/** Find the Map.Tiles area for a Room.Info area name. Your own cell's
 *  vnum wins when its name agrees (or Room.Info named no area), else
 *  the first area whose name matches. */
export function resolveArea(
  areaName: string | null,
  tiles: MapTilesAreas | null,
): { vnum: number; color: string | null } | null {
  if (!tiles) return null;
  const want = areaName?.toLowerCase();
  if (tiles.here !== null) {
    const own = tiles.areas[String(tiles.here)];
    if (own && (want === undefined || own.name?.toLowerCase() === want)) {
      return { vnum: tiles.here, color: own.color };
    }
  }
  if (want === undefined) return null;
  for (const [vnum, area] of Object.entries(tiles.areas)) {
    const n = Number(vnum);
    if (area.name?.toLowerCase() === want && Number.isFinite(n)) {
      return { vnum: n, color: area.color };
    }
  }
  return null;
}

function withArea(base: RoomInfoBase | null, tiles: MapTilesAreas | null): RoomInfo | null {
  if (!base) return null;
  const area = resolveArea(base.area, tiles);
  return { ...base, areaVnum: area?.vnum ?? null, areaColor: area?.color ?? null };
}

/** What the store keeps. The panes read `info` and `people`. */
interface RoomStoreState extends RoomState {
  /** The last Room.Info, before the Map.Tiles area. */
  base: RoomInfoBase | null;
  /** The areas of the last Map.Tiles. */
  tiles: MapTilesAreas | null;
}

const store = createSessionStore<RoomStoreState, RoomState>({
  state: { base: null, tiles: null, info: null, people: [] },
  packages: {
    'Room.Info': (state, data) => {
      const base = parseRoomInfo(data);
      return base ? { ...state, base, info: withArea(base, state.tiles) } : state;
    },
    'Room.Chars': (state, data) => ({ ...state, people: parsePeople(data) }),
    'Map.Tiles': (state, data) => {
      const tiles = parseMapAreas(data);
      if (!tiles) return state;
      // Map.Tiles comes with every step. It replaces the room only when
      // it changes which area the room resolves to.
      const info = withArea(state.base, tiles);
      const prev = state.info;
      return info && (info.areaVnum !== prev?.areaVnum || info.areaColor !== prev?.areaColor)
        ? { ...state, tiles, info }
        : { ...state, tiles };
    },
  },
  connection: (state) => state,
  view: (state, last) =>
    last?.info === state.info && last.people === state.people
      ? last
      : { info: state.info, people: state.people },
});

export const startRoomStore = store.start;
export const getRoom = store.get;
export const useRoom = store.use;
/** The room of the session `session` names, as the panes read it. */
export const getRoomOf = (session: number): RoomInfo | null => store.stateOf(session).info;
/** Hear each change to a session's room, with that session. */
export const subscribeRoomOf = store.subscribeStates;
