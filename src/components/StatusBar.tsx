import { useEffect, useRef, useState } from 'react';
import {
  getTarget,
  onGmcp,
  onState,
  onTarget,
  onTick,
  type TickPayload,
} from '../lib/session';

interface Vitals {
  hp: number;
  maxhp: number;
  mana: number;
  maxmana: number;
  move: number;
  maxmove: number;
}

interface VitalDeltas {
  hp: number | null;
  mana: number | null;
  move: number | null;
}

interface RoomInfo {
  name: string;
  exits: string[];
}

interface CombatState {
  name: string;
  hp?: number;
  condition?: string;
}

function formatClock(date: Date): string {
  const hh = String(date.getHours()).padStart(2, '0');
  const mm = String(date.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

function num(value: unknown, fallback: number): number {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string') {
    const n = Number(value);
    if (Number.isFinite(n)) return n;
  }
  return fallback;
}

function asNumber(value: unknown): number | undefined {
  if (typeof value === 'number') return Number.isFinite(value) ? value : undefined;
  if (typeof value === 'string') {
    const n = Number(value);
    return Number.isFinite(n) ? n : undefined;
  }
  return undefined;
}

function pct(current: number, max: number): number {
  if (max <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((current / max) * 100)));
}

function colorForPct(value: number): string {
  if (value >= 80) return '#87a987';
  if (value >= 60) return '#e6c384';
  if (value >= 40) return '#d99a6c';
  if (value >= 20) return '#e46876';
  return '#7d1d1d';
}

function tintForFill(hex: string): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  const k = 0.25;
  return `rgb(${Math.round(r * k)}, ${Math.round(g * k)}, ${Math.round(b * k)})`;
}

function extractCombat(data: unknown): CombatState | null {
  if (!data || typeof data !== 'object') return null;
  const obj = data as Record<string, unknown>;
  const name = typeof obj.target === 'string' ? obj.target.trim() : '';
  if (!name) return null;
  const out: CombatState = { name };
  const hp = asNumber(obj.hp_pct);
  if (hp !== undefined) out.hp = Math.max(0, Math.min(100, Math.round(hp)));
  if (typeof obj.condition === 'string' && obj.condition.trim()) {
    out.condition = obj.condition.trim();
  }
  return out;
}

function parseRoomInfo(data: unknown): RoomInfo | null {
  if (!data || typeof data !== 'object') return null;
  const obj = data as Record<string, unknown>;
  const name = typeof obj.name === 'string' ? obj.name : '';
  if (!name) return null;
  const exitsRaw = obj.exits;
  let exits: string[] = [];
  if (exitsRaw && typeof exitsRaw === 'object' && !Array.isArray(exitsRaw)) {
    exits = Object.keys(exitsRaw as Record<string, unknown>);
  } else if (Array.isArray(exitsRaw)) {
    exits = exitsRaw.filter((e): e is string => typeof e === 'string');
  } else if (typeof exitsRaw === 'string') {
    exits = exitsRaw.split(/[\s,]+/).filter(Boolean);
  }
  return { name, exits };
}

const COMPASS_ORDER: Record<string, number> = {
  n: 0,
  north: 0,
  e: 1,
  east: 1,
  s: 2,
  south: 2,
  w: 3,
  west: 3,
  u: 4,
  up: 4,
  d: 5,
  down: 5,
};

function compactExits(exits: string[]): string {
  const seen = new Set<string>();
  return exits
    .map((e) => e.trim().toLowerCase())
    .filter((e) => e.length > 0)
    .map((e) => e[0])
    .filter((c) => {
      if (seen.has(c)) return false;
      seen.add(c);
      return true;
    })
    .sort((a, b) => (COMPASS_ORDER[a] ?? 99) - (COMPASS_ORDER[b] ?? 99))
    .join('');
}

const NO_DELTAS: VitalDeltas = { hp: null, mana: null, move: null };

export function StatusBar() {
  const [now, setNow] = useState(() => new Date());
  const [vitals, setVitals] = useState<Vitals | null>(null);
  const [deltas, setDeltas] = useState<VitalDeltas>(NO_DELTAS);
  const [combat, setCombat] = useState<CombatState | null>(null);
  const [userTarget, setUserTarget] = useState<string | null>(null);
  const [room, setRoom] = useState<RoomInfo | null>(null);
  // Tintin-style tick display: counts UP real seconds since the last
  // World.Time hour change. Resets only on hour change, not on the
  // backend's local-interval auto-fire (which drifts ahead of or
  // behind the MUD's actual tick). Matches main's behavior verbatim.
  const [tick, setTick] = useState<TickPayload | null>(null);
  const [tickSecs, setTickSecs] = useState(0);
  const tickResetAtRef = useRef<number>(Date.now());
  const prevHourRef = useRef<number | string | null>(null);
  // Snapshot of the most-recent vitals; used to compute deltas on
  // tick fires. Kept in a ref so the GMCP handler that updates
  // vitals doesn't need to depend on it.
  const vitalsSnapRef = useRef<Vitals | null>(null);

  useEffect(() => {
    const wallTick = () => setNow(new Date());
    wallTick();
    const id = window.setInterval(wallTick, 15_000);
    return () => window.clearInterval(id);
  }, []);

  // 4Hz tick-seconds ticker — keeps the display smooth even when the
  // browser throttles longer timers under inactive-tab heuristics.
  useEffect(() => {
    const id = window.setInterval(() => {
      setTickSecs(Math.floor((Date.now() - tickResetAtRef.current) / 1000));
    }, 250);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let unsubTarget: (() => void) | undefined;
    let unsubTick: (() => void) | undefined;
    let cancelled = false;

    void getTarget()
      .then((snap) => {
        if (cancelled) return;
        setUserTarget(snap.name);
      })
      .catch(() => undefined);

    onGmcp((payload) => {
      if (payload.package === 'Char.Vitals') {
        const data = payload.data ?? {};
        const next: Vitals = {
          hp: num(data.hp, 0),
          maxhp: num(data.maxhp, 0),
          mana: num(data.mana, 0),
          maxmana: num(data.maxmana, 0),
          move: num(data.move, 0),
          maxmove: num(data.maxmove, 0),
        };
        setVitals(next);
        if (vitalsSnapRef.current === null) {
          vitalsSnapRef.current = next;
        }
      } else if (payload.package === 'Char.Combat') {
        setCombat(extractCombat(payload.data));
      } else if (payload.package === 'Room.Info') {
        setRoom(parseRoomInfo(payload.data));
      } else if (
        payload.package === 'World.Time' &&
        payload.data &&
        typeof payload.data === 'object'
      ) {
        // Direct port of main's reset logic: first World.Time after
        // connect just seeds the hour; any subsequent push with a
        // DIFFERENT hour value is a real tick and resets the counter.
        const incoming = payload.data as Record<string, unknown>;
        const hour = incoming.hour as number | string | undefined | null;
        if (hour !== undefined && hour !== null) {
          if (prevHourRef.current !== null && prevHourRef.current !== hour) {
            // Rebase the vitals snapshot too so the delta column
            // reflects per-tick movement.
            setVitals((curr) => {
              const prevSnap = vitalsSnapRef.current;
              if (curr) {
                if (prevSnap) {
                  setDeltas({
                    hp: curr.hp - prevSnap.hp,
                    mana: curr.mana - prevSnap.mana,
                    move: curr.move - prevSnap.move,
                  });
                }
                vitalsSnapRef.current = curr;
              }
              return curr;
            });
            tickResetAtRef.current = Date.now();
            setTickSecs(0);
          }
          prevHourRef.current = hour;
        }
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onTarget((payload) => {
      setUserTarget(payload.name);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubTarget = fn;
    });

    onTick((payload) => {
      // Backend tick state only — drives the enabled flag and the
      // interval used for the warn band. Reset of the counter
      // happens in the World.Time branch above (matches main).
      setTick(payload);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubTick = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setVitals(null);
        setDeltas(NO_DELTAS);
        setCombat(null);
        setUserTarget(null);
        setRoom(null);
        setTick(null);
        vitalsSnapRef.current = null;
        // Forget the last hour so the next connection's first
        // World.Time push is treated as the seed, not a hour-change.
        prevHourRef.current = null;
        tickResetAtRef.current = Date.now();
        setTickSecs(0);
      } else if (payload.kind === 'connected') {
        tickResetAtRef.current = Date.now();
        setTickSecs(0);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
    });

    return () => {
      cancelled = true;
      unsubGmcp?.();
      unsubState?.();
      unsubTarget?.();
      unsubTick?.();
    };
  }, []);

  // Tick is "warning" when within 5 seconds of the configured
  // interval (or already past it because the MUD's tick fired late
  // or was missed). Drives a yellow tint + a CSS pulse animation.
  const tickActive = !!tick?.enabled;
  const tickIntervalSec =
    tick?.interval_ms && tick.interval_ms > 0
      ? Math.max(1, Math.round(tick.interval_ms / 1000))
      : 30;
  const tickWarn = tickActive && tickSecs >= Math.max(0, tickIntervalSec - 5);

  return (
    <div className="statusbar">
      <div className="statusbar-left">
        {vitals && vitals.maxhp > 0 && (
          <VitalBar label="hp" cur={vitals.hp} max={vitals.maxhp} delta={deltas.hp} />
        )}
        {vitals && vitals.maxmana > 0 && (
          <VitalBar label="mana" cur={vitals.mana} max={vitals.maxmana} delta={deltas.mana} />
        )}
        {vitals && vitals.maxmove > 0 && (
          <VitalBar label="move" cur={vitals.move} max={vitals.maxmove} delta={deltas.move} />
        )}
        {tickActive && (
          <span className={`statusbar-tick${tickWarn ? ' statusbar-tick-warn' : ''}`}>
            <span className="statusbar-tick-label">tick</span>
            <span className="statusbar-tick-value">{tickSecs}s</span>
          </span>
        )}
        {combat && <CombatSeg combat={combat} />}
        {userTarget && !combat && (
          <span className="statusbar-target">
            <span className="statusbar-target-label">tar</span>
            <span className="statusbar-target-value">{userTarget}</span>
          </span>
        )}
      </div>
      <div className="statusbar-center">
        {room && (
          <span className="statusbar-room">
            <span className="statusbar-room-name">{room.name}</span>
            {room.exits.length > 0 && (
              <span className="statusbar-room-exits">[{compactExits(room.exits)}]</span>
            )}
          </span>
        )}
      </div>
      <div className="statusbar-right">
        <span className="statusbar-clock">{formatClock(now)}</span>
      </div>
    </div>
  );
}

function VitalBar({
  label,
  cur,
  max,
  delta,
}: {
  label: string;
  cur: number;
  max: number;
  delta: number | null;
}) {
  const value = pct(cur, max);
  const fill = colorForPct(value);
  // Text sits centered horizontally; once the fill drops below 50%,
  // the numbers are no longer over the colored bar and the dark tint
  // disappears into the dark track. Flip to the bright theme text
  // color in that range so the numbers stay readable.
  const textColor = value >= 50 ? tintForFill(fill) : 'var(--c-text-strong)';
  const showDelta = delta !== null && delta !== 0;
  const deltaPositive = (delta ?? 0) > 0;
  return (
    <div className="statusbar-bar">
      <span className="statusbar-bar-label">{label}</span>
      <span className="statusbar-bar-percent" style={{ color: fill }}>
        {value}%
      </span>
      <span className="statusbar-bar-track">
        <span
          className="statusbar-bar-fill"
          style={{ width: `${value}%`, background: fill }}
          aria-hidden="true"
        />
        <span className="statusbar-bar-text" style={{ color: textColor }}>
          {cur}/{max}
        </span>
      </span>
      {showDelta && (
        <span
          className={`statusbar-bar-delta${deltaPositive ? ' statusbar-bar-delta-up' : ' statusbar-bar-delta-down'}`}
        >
          {deltaPositive ? '+' : ''}
          {delta}
        </span>
      )}
    </div>
  );
}

function CombatSeg({ combat }: { combat: CombatState }) {
  const hp = combat.hp;
  const fill = hp !== undefined ? colorForPct(hp) : '#7aa89f';
  return (
    <div className="statusbar-combat">
      <span className="statusbar-combat-swords" aria-hidden="true">
        ⚔
      </span>
      <span className="statusbar-combat-name">{combat.name}</span>
      {hp !== undefined ? (
        <span className="statusbar-bar-track" style={{ width: 80 }}>
          <span
            className="statusbar-bar-fill"
            style={{ width: `${hp}%`, background: fill }}
            aria-hidden="true"
          />
          <span className="statusbar-bar-text" style={{ color: tintForFill(fill) }}>
            {hp}%
          </span>
        </span>
      ) : (
        <span className="statusbar-combat-unknown">--</span>
      )}
      {combat.condition && (
        <span className="statusbar-combat-condition">{combat.condition}</span>
      )}
    </div>
  );
}
