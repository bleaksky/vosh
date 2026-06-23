import { useEffect, useMemo, useState } from 'react';
import { onGmcpPackage, onState, onTarget } from '../lib/session';
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
  npc?: number | string | boolean;
}

interface RoomItem {
  name?: string;
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
      '#585858',
      '#c4746e',
      '#8a9a7b',
      '#c4b28a',
      '#8ba4b0',
      '#a292a3',
      '#8ea4a2',
      '#a4a7a4',
      '#5c6066',
      '#e46876',
      '#87a987',
      '#e6c384',
      '#7fb4ca',
      '#938aa9',
      '#7aa89f',
      '#c5c9c7',
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

// Collapse duplicate entries by name and emit `(N) name` for any
// group with count > 1. Matches how the MUD itself renders the room
// description ("(21) A pair of crystal earrings lie here"). Preserves
// first-occurrence order so the strip stays stable across pushes.
interface RoomGroup<T> {
  name: string;
  count: number;
  sample: T;
  /// 1-based original indices into the source array. Used by the chars
  /// list so the target-marker (backend resolves target_idx against the
  /// pre-grouped Room.Chars order) still lights up the right chip after
  /// duplicates collapse.
  positions: number[];
}

function groupByName<T extends { name?: string }>(items: T[]): RoomGroup<T>[] {
  const order: string[] = [];
  const map = new Map<string, RoomGroup<T>>();
  items.forEach((it, idx) => {
    const name = typeof it.name === 'string' ? it.name : '';
    if (!name) return;
    const pos = idx + 1;
    const existing = map.get(name);
    if (existing) {
      existing.count += 1;
      existing.positions.push(pos);
    } else {
      map.set(name, { name, count: 1, sample: it, positions: [pos] });
      order.push(name);
    }
  });
  return order.map((n) => map.get(n)!);
}

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

interface Props {
  /** Layout variant. `strip` (default) is a single horizontal line —
   *  the original tintin-inspired prompt strip that lives in top /
   *  bottom zones. `column` wraps content onto multiple lines and
   *  drops the horizontal scrollbar; used when placed in a left /
   *  right side panel where width is constrained and height is
   *  available. */
  variant?: 'strip' | 'column';
}

// Single horizontal strip above the terminal: area · room (vnum) ·
// terrain · [exits] · here: chars · items: items. Replaces the 5-row
// stack from the old RoomInfoBar so the room context reads in one
// glance without consuming vertical space.
export function RoomStrip({ variant = 'strip' }: Props = {}) {
  const [room, setRoom] = useState<RoomInfo | null>(null);
  const [areas, setAreas] = useState<AreaMap | null>(null);
  const [chars, setChars] = useState<RoomChar[]>([]);
  const [items, setItems] = useState<RoomItem[]>([]);
  const [targetIdx, setTargetIdx] = useState<number | null>(null);

  useEffect(() => {
    // Phase 4: one subscription per GMCP package. Each handler only
    // runs on its own packets, instead of every Char.Vitals etc.
    // dispatching to this listener and being filtered out.
    const unsubs: Array<() => void> = [];
    let unsubState: (() => void) | undefined;
    let unsubTarget: (() => void) | undefined;
    let cancelled = false;
    const track = (p: Promise<() => void>) =>
      p.then((fn) => {
        if (cancelled) fn();
        else unsubs.push(fn);
      });

    track(
      onGmcpPackage<RoomInfo>('Room.Info', (data) => {
        if (data && typeof data === 'object') setRoom(data);
      }),
    );
    track(
      onGmcpPackage<{ areas?: AreaMap }>('Map.Tiles', (data) => {
        if (data && typeof data === 'object' && data.areas && typeof data.areas === 'object') {
          setAreas(data.areas);
        }
      }),
    );
    track(
      onGmcpPackage<RoomChar[]>('Room.Chars', (data) => {
        setChars(Array.isArray(data) ? data : []);
      }),
    );
    track(
      onGmcpPackage<RoomItem[]>('Room.Items', (data) => {
        setItems(Array.isArray(data) ? data : []);
      }),
    );

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setRoom(null);
        setAreas(null);
        setChars([]);
        setItems([]);
        setTargetIdx(null);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
    });

    onTarget((payload) => {
      setTargetIdx(payload.room_idx);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubTarget = fn;
    });

    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
      unsubState?.();
      unsubTarget?.();
    };
  }, []);

  // Group only when the char / item lists change, not on unrelated
  // re-renders (e.g. a Map.Tiles area update the strip mostly ignores).
  // Kept above the early return below to satisfy the rules of hooks.
  const charGroups = useMemo(() => groupByName(chars), [chars]);
  const itemGroups = useMemo(() => groupByName(items), [items]);

  // Slot stays reserved at fixed height even when empty so xterm
  // doesn't reflow when room info first arrives. Matches the
  // BottomHUD's same trick on the stats row.
  if (!room || !room.name) {
    return <div className="room-strip is-empty" aria-hidden="true" />;
  }
  const areaColor = room.area ? resolveAreaColor(room.area, areas) : FALLBACK_AREA_COLOR;
  const exits = formatExits(room.exits);
  const sectorColor = room.terrain ? sectorForTerrain(room.terrain).halo : undefined;

  return (
    <div className={`room-strip-wrap${variant === 'column' ? ' room-strip-wrap-column' : ''}`}>
      <div
        className={`room-strip${variant === 'column' ? ' room-strip-column' : ''}`}
        aria-label="current room"
      >
        {room.area && (
          <span className="room-strip-area" style={{ color: areaColor }}>
            {room.area}
          </span>
        )}
        <span className="room-strip-bullet" aria-hidden="true">
          ·
        </span>
        <span className="room-strip-name">{room.name}</span>
        {room.num !== undefined && room.num !== null && (
          <span className="room-strip-vnum">#{room.num}</span>
        )}
        {room.terrain && (
          <>
            <span className="room-strip-bullet" aria-hidden="true">
              ·
            </span>
            <span className="room-strip-terrain" style={{ color: sectorColor }}>
              {room.terrain}
            </span>
          </>
        )}
        {exits && (
          <>
            <span className="room-strip-bullet" aria-hidden="true">
              ·
            </span>
            <span className="room-strip-exits">[{exits}]</span>
          </>
        )}
        {charGroups.length > 0 && (
          <>
            <span className="room-strip-bullet" aria-hidden="true">
              ·
            </span>
            <span className="room-strip-list-label">here</span>
            <span className="room-strip-list">
              {charGroups.map((grp, i) => {
                const isTarget = targetIdx !== null && grp.positions.includes(targetIdx);
                return (
                  <span
                    key={`${grp.name}-${i}`}
                    className={`room-strip-chip${isTarget ? ' is-target' : ''}`}
                    style={{ color: isNpc(grp.sample.npc) ? NPC_COLOR : PLAYER_COLOR }}
                  >
                    {isTarget && (
                      <span className="room-strip-chip-mark" aria-hidden="true">
                        ▶
                      </span>
                    )}
                    {grp.count > 1 && (
                      <span className="room-strip-count" aria-hidden="true">
                        ({grp.count})
                      </span>
                    )}
                    {grp.name}
                    {i < charGroups.length - 1 && (
                      <span className="room-strip-list-sep" aria-hidden="true">
                        ,
                      </span>
                    )}
                  </span>
                );
              })}
            </span>
          </>
        )}
        {itemGroups.length > 0 && (
          <>
            <span className="room-strip-bullet" aria-hidden="true">
              ·
            </span>
            <span className="room-strip-list-label">items</span>
            <span className="room-strip-list">
              {itemGroups.map((grp, i) => (
                <span
                  key={`${grp.name}-${i}`}
                  className="room-strip-chip"
                  style={{ color: ITEM_TYPE_COLORS[grp.sample.type ?? ''] ?? ITEM_DEFAULT_COLOR }}
                >
                  {grp.count > 1 && (
                    <span className="room-strip-count" aria-hidden="true">
                      ({grp.count})
                    </span>
                  )}
                  {grp.name}
                  {i < itemGroups.length - 1 && (
                    <span className="room-strip-list-sep" aria-hidden="true">
                      ,
                    </span>
                  )}
                </span>
              ))}
            </span>
          </>
        )}
        {/* End-pad provides 24px after the LAST content when the user
          has scrolled all the way right. Combined with the fade
          overlay below (which masks the right edge regardless of
          scroll position), the strip never visually crashes content
          into the panel border. */}
        <span className="room-strip-end-pad" aria-hidden="true" />
      </div>
      {/* Right-edge fade overlay. Lives OUTSIDE the scrollable
          .room-strip so it stays glued to the visible right edge
          regardless of how far the user has scrolled. The gradient
          fades the underlying text into the surface color over the
          last 28px so cut-off content reads as "trails off" instead
          of "hard-clipped mid-letter". */}
      <span className="room-strip-fade" aria-hidden="true" />
    </div>
  );
}
