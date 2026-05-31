import { useEffect, useRef, useState } from 'react';
import {
  DEFAULT_VITALS_CONFIG,
  getUiConfig,
  onGmcp,
  onState,
  subscribeVitalsConfigChanged,
  type VitalsConfig,
} from '../lib/session';
import { useTickState } from '../lib/useTickState';

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

// Stacked vitals — one row per hp/mana/move. The tick countdown
// is a separate panel (TickPanel below) so the user can hide vitals
// without losing the tick or vice versa. Each row is
// `label · bar (20 cells) · % · cur/max · delta`. Subscribes to
// Char.Vitals + World.Time; World.Time hour-change rebases the
// per-tick delta snapshot.
export function VitalsBar({ embedTick = false }: { embedTick?: boolean } = {}) {
  const [vitals, setVitals] = useState<Vitals | null>(null);
  const [deltas, setDeltas] = useState<VitalDeltas>(NO_DELTAS);
  const [config, setConfig] = useState<VitalsConfig>(DEFAULT_VITALS_CONFIG);
  const vitalsSnapRef = useRef<Vitals | null>(null);
  const prevHourRef = useRef<number | string | null>(null);

  // Vitals appearance config. Read once on mount then live-updated via
  // vosh://vitals-config-changed so Settings edits land without a
  // relaunch and the bar redraws on the next render.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    getUiConfig()
      .then((cfg) => {
        if (!cancelled) setConfig(cfg.vitals);
      })
      .catch(() => {});
    subscribeVitalsConfigChanged((next) => {
      if (!cancelled) setConfig(next);
    }).then((fn) => {
      if (cancelled) fn();
      else unsub = fn;
    });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
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
          }
          prevHourRef.current = hour;
        }
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setVitals(null);
        setDeltas(NO_DELTAS);
        vitalsSnapRef.current = null;
        prevHourRef.current = null;
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

  if (!vitals) return null;

  const segs: Array<{ label: string; cur: number; max: number; delta: number | null }> = [];
  if (vitals.maxhp > 0)
    segs.push({ label: 'hp', cur: vitals.hp, max: vitals.maxhp, delta: deltas.hp });
  if (vitals.maxmana > 0)
    segs.push({ label: 'mn', cur: vitals.mana, max: vitals.maxmana, delta: deltas.mana });
  if (vitals.maxmove > 0)
    segs.push({ label: 'mv', cur: vitals.move, max: vitals.maxmove, delta: deltas.move });
  if (segs.length === 0) return null;

  return (
    <div className={`vitals-bar${config.show_bar ? '' : ' vitals-bar-no-bar'}`} aria-label="vitals">
      {segs.map((s) => (
        <VitalRow key={s.label} config={config} {...s} />
      ))}
      {embedTick && <InlineTick />}
    </div>
  );
}

// Inline tick chip rendered at the right edge of a host panel
// (vitals, roomstrip, affects, statusbar) when the user picks one
// of the `in:*` zones for the tick. Returns null when inactive so
// hosts can drop it in unconditionally.
export function InlineTick({ className }: { className?: string }) {
  const { active, tickSecs, warn } = useTickState();
  if (!active) return null;
  return (
    <span className={`inline-tick${className ? ` ${className}` : ''}`} aria-label="tick">
      <span className="vitals-label">tick</span>
      <span className={`vitals-tick-value${warn ? ' is-warn' : ''}`}>{tickSecs}s</span>
    </span>
  );
}

// Stand-alone tick countdown panel. Reuses the vitals-bar grid so it
// renders as a single "tick · 12s" row consistent with the hp/mn/mv
// rows. Lives in the panel registry as its own movable element so the
// user can hide vitals without losing the tick.
export function TickPanel() {
  const { active, tickSecs, warn } = useTickState();
  if (!active) return null;
  return (
    <div className="vitals-bar tick-panel" aria-label="tick">
      <TickRow tickSecs={tickSecs} warn={warn} />
    </div>
  );
}

function VitalRow({
  label,
  cur,
  max,
  delta,
  config,
}: {
  label: string;
  cur: number;
  max: number;
  delta: number | null;
  config: VitalsConfig;
}) {
  const value = pct(cur, max);
  const fill = colorForVital(label, value);
  const total = Math.max(4, Math.min(60, config.bar_width));
  const filledCount = Math.round((value / 100) * total);
  const emptyCount = total - filledCount;
  const showDelta = config.show_delta && delta !== null && delta !== 0;
  const deltaPositive = (delta ?? 0) > 0;

  return (
    <div className="vitals-row">
      <span className="vitals-label">{label}</span>
      {config.show_bar && (
        <span className="vitals-glyphs" aria-hidden="true">
          {filledCount > 0 && (
            <span style={{ color: fill }}>{config.bar_filled.repeat(filledCount)}</span>
          )}
          {emptyCount > 0 && (
            <span className="vitals-empty">{config.bar_empty.repeat(emptyCount)}</span>
          )}
        </span>
      )}
      {config.show_percent && (
        <span className="vitals-percent" style={{ color: fill }}>
          {value}%
        </span>
      )}
      {config.show_numeric && (
        <span className="vitals-numeric">
          {cur}/{max}
        </span>
      )}
      {config.show_delta && (
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
      )}
    </div>
  );
}

function TickRow({ tickSecs, warn }: { tickSecs: number; warn: boolean }) {
  return (
    <div className="vitals-row">
      <span className="vitals-label">tick</span>
      <span className={`vitals-tick-value${warn ? ' is-warn' : ''}`}>{tickSecs}s</span>
    </div>
  );
}
