import { useEffect, useState } from 'react';
import { onGmcp, onState } from '../lib/session';
import { sectorForTerrain } from '../lib/mapPalette';

interface RoomInfo {
  name?: string;
  num?: number | string;
  area?: string;
  terrain?: string;
  exits?: Record<string, number | string>;
}

interface AreaInfo {
  name?: string;
  color?: string | number;
}

type AreaMap = Record<string, AreaInfo>;

interface RoomChar {
  name?: string;
  /// Aabahran ships either `1`/`"1"`/`"true"` for NPCs and absent or
  /// falsy for players.
  npc?: number | string | boolean;
}

interface RoomItem {
  name?: string;
  /// `money`, `weapon`, `armor`, `potion`, `food`, etc.
  type?: string;
}

const ANSI_256_CUBE = [0, 95, 135, 175, 215, 255];

function ansi256ToHex(idx: number): string {
  if (idx < 0 || idx > 255) return '#c5c9c7';
  if (idx >= 232) {
    const v = 8 + (idx - 232) * 10;
    const h = v.toString(16).padStart(2, '0');
    return `#${h}${h}${h}`;
  }
  if (idx < 16) {
    const named: string[] = [
      '#585858', '#c4746e', '#8a9a7b', '#c4b28a',
      '#8ba4b0', '#a292a3', '#8ea4a2', '#a4a7a4',
      '#5c6066', '#e46876', '#87a987', '#e6c384',
      '#7fb4ca', '#938aa9', '#7aa89f', '#c5c9c7',
    ];
    return named[idx] ?? '#c5c9c7';
  }
  const c = idx - 16;
  const r = Math.floor(c / 36);
  const g = Math.floor((c % 36) / 6);
  const b = c % 6;
  const toHex = (n: number) => n.toString(16).padStart(2, '0');
  return `#${toHex(ANSI_256_CUBE[r])}${toHex(ANSI_256_CUBE[g])}${toHex(ANSI_256_CUBE[b])}`;
}

const FALLBACK_AREA_COLOR = ansi256ToHex(220);

// Item-type → text color, lifted from the user's `aabahran_ui.tin`
// `panel_info` block so colors line up with their tintin client.
const ITEM_TYPE_COLORS: Record<string, string> = {
  money: ansi256ToHex(221),
  weapon: ansi256ToHex(203),
  armor: ansi256ToHex(110),
  potion: ansi256ToHex(141),
  food: ansi256ToHex(149),
};
const ITEM_DEFAULT_COLOR = ansi256ToHex(252);

const NPC_COLOR = ansi256ToHex(110);
const PLAYER_COLOR = ansi256ToHex(255);

function isNpc(v: RoomChar['npc']): boolean {
  return v === 1 || v === '1' || v === 'true' || v === true;
}

function resolveAreaColor(area: string, areas: AreaMap | null): string {
  if (!areas) return FALLBACK_AREA_COLOR;
  const info = areas[area];
  if (!info) return FALLBACK_AREA_COLOR;
  if (typeof info.color === 'number' && Number.isFinite(info.color)) {
    return ansi256ToHex(info.color);
  }
  if (typeof info.color === 'string' && info.color.length > 0) {
    if (info.color.startsWith('#')) return info.color;
    const n = Number(info.color);
    if (Number.isFinite(n)) return ansi256ToHex(n);
  }
  return FALLBACK_AREA_COLOR;
}

const EXIT_ORDER = ['north', 'east', 'south', 'west', 'up', 'down'];
const EXIT_LABELS: Record<string, string> = {
  north: 'N',
  east: 'E',
  south: 'S',
  west: 'W',
  up: 'U',
  down: 'D',
};

function formatExits(exits: Record<string, number | string> | undefined): string {
  if (!exits) return '';
  const out: string[] = [];
  for (const dir of EXIT_ORDER) {
    if (exits[dir] !== undefined && exits[dir] !== null && exits[dir] !== 0) {
      out.push(EXIT_LABELS[dir]);
    }
  }
  return out.join(' ');
}

export function RoomInfoBar() {
  const [room, setRoom] = useState<RoomInfo | null>(null);
  const [areas, setAreas] = useState<AreaMap | null>(null);
  const [chars, setChars] = useState<RoomChar[]>([]);
  const [items, setItems] = useState<RoomItem[]>([]);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let cancelled = false;

    onGmcp((payload) => {
      if (
        payload.package === 'Room.Info' &&
        payload.data &&
        typeof payload.data === 'object'
      ) {
        setRoom(payload.data as RoomInfo);
        return;
      }
      if (
        payload.package === 'Map.Tiles' &&
        payload.data &&
        typeof payload.data === 'object'
      ) {
        const data = payload.data as { areas?: AreaMap };
        if (data.areas && typeof data.areas === 'object') {
          setAreas(data.areas);
        }
        return;
      }
      if (payload.package === 'Room.Chars') {
        // Aabahran sends a top-level array. Replace state wholesale on
        // each push so chars who left the room don't linger.
        setChars(Array.isArray(payload.data) ? (payload.data as RoomChar[]) : []);
        return;
      }
      if (payload.package === 'Room.Items') {
        setItems(Array.isArray(payload.data) ? (payload.data as RoomItem[]) : []);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setRoom(null);
        setAreas(null);
        setChars([]);
        setItems([]);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
    });

    return () => {
      cancelled = true;
      unsubGmcp?.();
      unsubState?.();
    };
  }, []);

  if (!room || !room.name) return null;
  const areaColor = room.area ? resolveAreaColor(room.area, areas) : FALLBACK_AREA_COLOR;
  const exits = formatExits(room.exits);
  const sectorColor = room.terrain ? sectorForTerrain(room.terrain).halo : undefined;
  const namedChars = chars.filter((c) => typeof c.name === 'string' && c.name.length > 0);
  const namedItems = items.filter((i) => typeof i.name === 'string' && i.name.length > 0);

  return (
    <div className="room-info-bar" aria-label="current room">
      {room.area && (
        <div className="room-info-area" style={{ color: areaColor }}>
          {room.area}
        </div>
      )}
      <div className="room-info-line">
        <span className="room-info-name">{room.name}</span>
        {room.num !== undefined && room.num !== null && (
          <span className="room-info-vnum">#{room.num}</span>
        )}
      </div>
      {(room.terrain || exits) && (
        <div className="room-info-sub">
          {room.terrain && (
            <>
              <span className="room-info-bullet">·</span>
              <span className="room-info-terrain" style={{ color: sectorColor }}>
                {room.terrain}
              </span>
              <span className="room-info-bullet">·</span>
            </>
          )}
          {exits && <span className="room-info-exits">[{exits}]</span>}
        </div>
      )}
      {namedChars.length > 0 && (
        <div className="room-info-list" aria-label="here">
          <span className="room-info-list-label">here</span>
          {namedChars.map((c, i) => (
            <span
              key={`${c.name}-${i}`}
              className="room-info-chip"
              style={{ color: isNpc(c.npc) ? NPC_COLOR : PLAYER_COLOR }}
            >
              {c.name}
            </span>
          ))}
        </div>
      )}
      {namedItems.length > 0 && (
        <div className="room-info-list" aria-label="items">
          <span className="room-info-list-label">items</span>
          {namedItems.map((it, i) => (
            <span
              key={`${it.name}-${i}`}
              className="room-info-chip"
              style={{ color: ITEM_TYPE_COLORS[it.type ?? ''] ?? ITEM_DEFAULT_COLOR }}
            >
              {it.name}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
