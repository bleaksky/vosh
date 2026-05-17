import { useEffect, useRef, useState } from 'react';
import { getTarget, onGmcp, onState, onTarget } from '../lib/session';

interface Vitals {
  hp: number;
  maxhp: number;
  mana: number;
  maxmana: number;
  move: number;
  maxmove: number;
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

// Aabahran's Char.Combat payload: { target, hp_pct, condition }. An
// empty object signals the target is gone.
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

export function StatusBar() {
  const [now, setNow] = useState(() => new Date());
  const [vitals, setVitals] = useState<Vitals | null>(null);
  const [combat, setCombat] = useState<CombatState | null>(null);
  const [userTarget, setUserTarget] = useState<string | null>(null);
  const [room, setRoom] = useState<RoomInfo | null>(null);
  // Tintin-style tick display: counts UP real seconds since the last
  // World.Time hour change. Reset only on hour change, not on the
  // backend's interval-based auto-fire (which drifts ahead of or
  // behind the MUD's actual tick). Matches the prior build's behavior.
  const [tickSecs, setTickSecs] = useState(0);
  const [tickActive, setTickActive] = useState(false);
  const tickResetAtRef = useRef<number>(Date.now());
  const prevHourRef = useRef<number | string | null>(null);

  useEffect(() => {
    const wallTick = () => setNow(new Date());
    wallTick();
    const id = window.setInterval(wallTick, 15_000);
    return () => window.clearInterval(id);
  }, []);

  // 4Hz ticker for the tick-seconds counter. setInterval at 250ms so
  // the display stays responsive even when the browser throttles
  // longer timers under inactive-tab heuristics.
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
    let cancelled = false;

    // Seed tar selection from the backend on mount.
    void getTarget()
      .then((snap) => {
        if (cancelled) return;
        setUserTarget(snap.name);
      })
      .catch(() => undefined);

    onGmcp((payload) => {
      if (payload.package === 'Char.Vitals') {
        const data = payload.data ?? {};
        setVitals({
          hp: num(data.hp, 0),
          maxhp: num(data.maxhp, 0),
          mana: num(data.mana, 0),
          maxmana: num(data.maxmana, 0),
          move: num(data.move, 0),
          maxmove: num(data.maxmove, 0),
        });
      } else if (payload.package === 'Char.Combat') {
        setCombat(extractCombat(payload.data));
      } else if (payload.package === 'Room.Info') {
        setRoom(parseRoomInfo(payload.data));
      } else if (payload.package === 'World.Time' && payload.data && typeof payload.data === 'object') {
        // First World.Time after connect just seeds the hour. Each
        // subsequent push with a different hour value is the tick
        // fire — that's the canonical reset signal.
        const incoming = payload.data as Record<string, unknown>;
        const hour = incoming.hour;
        setTickActive(true);
        if (hour !== undefined && hour !== null) {
          if (prevHourRef.current !== null && prevHourRef.current !== hour) {
            tickResetAtRef.current = Date.now();
            setTickSecs(0);
          }
          prevHourRef.current = hour as number | string;
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

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setVitals(null);
        setCombat(null);
        setUserTarget(null);
        setRoom(null);
        setTickActive(false);
        prevHourRef.current = null;
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
    };
  }, []);

  return (
    <div className="statusbar">
      <div className="statusbar-left">
        {vitals && vitals.maxhp > 0 && (
          <VitalBar label="hp" cur={vitals.hp} max={vitals.maxhp} />
        )}
        {vitals && vitals.maxmana > 0 && (
          <VitalBar label="mana" cur={vitals.mana} max={vitals.maxmana} />
        )}
        {vitals && vitals.maxmove > 0 && (
          <VitalBar label="move" cur={vitals.move} max={vitals.maxmove} />
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
        {tickActive && (
          <span className="statusbar-tick">
            <span className="statusbar-tick-label">tick</span>
            <span className="statusbar-tick-value">{tickSecs}s</span>
          </span>
        )}
        <span className="statusbar-clock">{formatClock(now)}</span>
      </div>
    </div>
  );
}

function VitalBar({ label, cur, max }: { label: string; cur: number; max: number }) {
  const value = pct(cur, max);
  const fill = colorForPct(value);
  const textColor = tintForFill(fill);
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
