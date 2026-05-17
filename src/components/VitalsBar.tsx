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

// Glyph cells per bar. Total width is tight so all three vitals plus
// the tick fit on a single line. Slanted parallelograms (▰/▱) match
// the compaction-progress aesthetic the user pointed at.
const GLYPHS_TOTAL = 8;
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

function colorForPct(value: number): string {
  if (value >= 80) return '#87a987';
  if (value >= 60) return '#e6c384';
  if (value >= 40) return '#d99a6c';
  if (value >= 20) return '#e46876';
  return '#7d1d1d';
}

// Single-line vitals row: hp / mana / move as slanted glyph
// progress bars side by side, plus the World.Time tick counter at
// the end. Subscribes to Char.Vitals, World.Time, and the backend
// tick stream; World.Time hour changes both rebase the per-tick
// vitals snapshot AND reset the tick seconds counter.
export function VitalsBar() {
  const [vitals, setVitals] = useState<Vitals | null>(null);
  const [deltas, setDeltas] = useState<VitalDeltas>(NO_DELTAS);
  const [tick, setTick] = useState<TickPayload | null>(null);
  const [tickSecs, setTickSecs] = useState(0);
  const vitalsSnapRef = useRef<Vitals | null>(null);
  const prevHourRef = useRef<number | string | null>(null);
  const tickResetAtRef = useRef<number>(Date.now());

  // 4Hz tick-seconds ticker for the on-row counter.
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
        <VitalSegment key={s.label} {...s} />
      ))}
      {tickActive && (
        <span className="vitals-seg vitals-seg-tick">
          <span className="vitals-seg-top">
            <span className="vitals-seg-label">tick</span>
            <span className={`vitals-seg-tick-value${tickWarn ? ' is-warn' : ''}`}>
              {tickSecs}s
            </span>
          </span>
        </span>
      )}
    </div>
  );
}

function VitalSegment({
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
  const filledCount = Math.round((value / 100) * GLYPHS_TOTAL);
  const emptyCount = GLYPHS_TOTAL - filledCount;
  const showDelta = delta !== null && delta !== 0;
  const deltaPositive = (delta ?? 0) > 0;

  return (
    <span className="vitals-seg">
      <span className="vitals-seg-label">{label}</span>
      <span className="vitals-seg-bracket vitals-seg-bracket-l" aria-hidden="true">
        「
      </span>
      <span className="vitals-seg-glyphs" aria-hidden="true">
        {filledCount > 0 && (
          <span style={{ color: fill }}>{FILLED.repeat(filledCount)}</span>
        )}
        {emptyCount > 0 && (
          <span className="vitals-seg-empty">{EMPTY.repeat(emptyCount)}</span>
        )}
      </span>
      <span className="vitals-seg-bracket vitals-seg-bracket-r" aria-hidden="true">
        」
      </span>
      <span className="vitals-seg-percent" style={{ color: fill }}>
        {value}%
      </span>
      <span className="vitals-seg-numeric">
        {cur}/{max}
        {showDelta && (
          <span
            className={`vitals-seg-delta-inline${deltaPositive ? ' vitals-seg-delta-up' : ' vitals-seg-delta-down'}`}
          >
            {' '}
            {deltaPositive ? '+' : ''}
            {delta}
          </span>
        )}
      </span>
    </span>
  );
}
