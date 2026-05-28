import { useEffect, useRef, useState } from 'react';
import { onGmcp, onState, onTick, type TickPayload } from '../lib/session';

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

const NO_DELTAS: VitalDeltas = { hp: null, mana: null, move: null };

const GLYPHS_TOTAL = 20;
const FILLED = '▰';
const EMPTY = '▱';

function num(value: unknown, fallback: number): number {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string') {
    const n = Number(value);
    if (Number.isFinite(n)) return n;
  }
  return fallback;
}

function pct(current: number, max: number): number {
  if (max <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((current / max) * 100)));
}

// Each vital has its own healthy-color identity (hp green, mn
// blue, mv warm-orange) but every ramp converges on red as the
// bar drains, so the eye picks up both "which vital" and "how
// urgent" from the color alone. Intermediate stops avoid muddy
// straight-line interpolation through unflattering midpoints.
type Stop = [pct: number, rgb: [number, number, number]];

const VITAL_RAMPS: Record<string, Stop[]> = {
  // green -> yellow -> orange -> red
  hp: [
    [0, [228, 104, 118]],
    [25, [217, 154, 108]],
    [55, [230, 195, 132]],
    [100, [135, 169, 135]],
  ],
  // blue -> teal -> purple -> red
  mn: [
    [0, [228, 104, 118]],
    [30, [192, 130, 168]],
    [65, [134, 153, 188]],
    [100, [127, 180, 202]],
  ],
  // orange -> deep red
  mv: [
    [0, [228, 104, 118]],
    [60, [220, 130, 100]],
    [100, [217, 154, 108]],
  ],
};

function colorForVital(label: string, value: number): string {
  const ramp = VITAL_RAMPS[label] ?? VITAL_RAMPS.hp;
  const v = Math.max(0, Math.min(100, value));
  let lower = ramp[0];
  let upper = ramp[ramp.length - 1];
  for (let i = 0; i < ramp.length - 1; i++) {
    if (v >= ramp[i][0] && v <= ramp[i + 1][0]) {
      lower = ramp[i];
      upper = ramp[i + 1];
      break;
    }
  }
  const range = upper[0] - lower[0];
  const t = range > 0 ? (v - lower[0]) / range : 0;
  const lerp = (a: number, b: number) => Math.round(a + (b - a) * t);
  const r = lerp(lower[1][0], upper[1][0]);
  const g = lerp(lower[1][1], upper[1][1]);
  const b = lerp(lower[1][2], upper[1][2]);
  return `rgb(${r}, ${g}, ${b})`;
}

// Stacked vitals — one row per hp/mana/move, plus the tick. Each
// row is `label · bar (20 cells) · % · cur/max · delta`. Columns
// line up across all rows via grid auto-sizing. Subscribes to
// Char.Vitals + World.Time + onTick; World.Time hour-change
// rebases the per-tick delta snapshot and resets the tick counter.
export function VitalsBar() {
  const [vitals, setVitals] = useState<Vitals | null>(null);
  const [deltas, setDeltas] = useState<VitalDeltas>(NO_DELTAS);
  const [tick, setTick] = useState<TickPayload | null>(null);
  const [tickSecs, setTickSecs] = useState(0);
  const vitalsSnapRef = useRef<Vitals | null>(null);
  const prevHourRef = useRef<number | string | null>(null);
  const tickResetAtRef = useRef<number>(Date.now());

  useEffect(() => {
    const id = window.setInterval(() => {
      setTickSecs(Math.floor((Date.now() - tickResetAtRef.current) / 1000));
    }, 250);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let unsubTick: (() => void) | undefined;
    let cancelled = false;

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
        const snap = vitalsSnapRef.current;
        if (snap === null) {
          vitalsSnapRef.current = next;
          setDeltas(NO_DELTAS);
        } else {
          setDeltas({
            hp: next.hp - snap.hp,
            mana: next.mana - snap.mana,
            move: next.move - snap.move,
          });
        }
      } else if (
        payload.package === 'World.Time' &&
        payload.data &&
        typeof payload.data === 'object'
      ) {
        const incoming = payload.data as Record<string, unknown>;
        const hour = incoming.hour as number | string | undefined | null;
        if (hour !== undefined && hour !== null) {
          if (prevHourRef.current !== null && prevHourRef.current !== hour) {
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

    onTick((payload) => {
      setTick(payload);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubTick = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setVitals(null);
        setDeltas(NO_DELTAS);
        setTick(null);
        vitalsSnapRef.current = null;
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
      unsubTick?.();
    };
  }, []);

  const tickActive = !!tick?.enabled;
  const tickIntervalSec =
    tick?.interval_ms && tick.interval_ms > 0
      ? Math.max(1, Math.round(tick.interval_ms / 1000))
      : 30;
  const tickWarn = tickActive && tickSecs >= Math.max(0, tickIntervalSec - 5);

  if (!vitals && !tickActive) return null;

  const segs: Array<{ label: string; cur: number; max: number; delta: number | null }> = [];
  if (vitals) {
    if (vitals.maxhp > 0)
      segs.push({ label: 'hp', cur: vitals.hp, max: vitals.maxhp, delta: deltas.hp });
    if (vitals.maxmana > 0)
      segs.push({ label: 'mn', cur: vitals.mana, max: vitals.maxmana, delta: deltas.mana });
    if (vitals.maxmove > 0)
      segs.push({ label: 'mv', cur: vitals.move, max: vitals.maxmove, delta: deltas.move });
  }

  return (
    <div className="vitals-bar" aria-label="vitals">
      {segs.map((s) => (
        <VitalRow key={s.label} {...s} />
      ))}
      {tickActive && <TickRow tickSecs={tickSecs} warn={tickWarn} />}
    </div>
  );
}

function VitalRow({
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
  const fill = colorForVital(label, value);
  const filledCount = Math.round((value / 100) * GLYPHS_TOTAL);
  const emptyCount = GLYPHS_TOTAL - filledCount;
  const showDelta = delta !== null && delta !== 0;
  const deltaPositive = (delta ?? 0) > 0;

  return (
    <>
      <span className="vitals-label">{label}</span>
      <span className="vitals-glyphs" aria-hidden="true">
        {filledCount > 0 && <span style={{ color: fill }}>{FILLED.repeat(filledCount)}</span>}
        {emptyCount > 0 && <span className="vitals-empty">{EMPTY.repeat(emptyCount)}</span>}
      </span>
      <span className="vitals-percent" style={{ color: fill }}>
        {value}%
      </span>
      <span className="vitals-numeric">
        {cur}/{max}
      </span>
      <span className="vitals-delta-slot">
        {showDelta && (
          <span
            className={`vitals-delta${deltaPositive ? ' vitals-delta-up' : ' vitals-delta-down'}`}
          >
            {deltaPositive ? '+' : ''}
            {delta}
          </span>
        )}
      </span>
    </>
  );
}

function TickRow({ tickSecs, warn }: { tickSecs: number; warn: boolean }) {
  return (
    <>
      <span className="vitals-label">tick</span>
      <span className={`vitals-tick-value${warn ? ' is-warn' : ''}`}>{tickSecs}s</span>
    </>
  );
}
