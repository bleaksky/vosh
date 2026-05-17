import { useEffect, useState } from 'react';
import { onGmcp, onState } from '../lib/session';

interface Vitals {
  hp: number;
  maxhp: number;
  mana: number;
  maxmana: number;
  move: number;
  maxmove: number;
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

function pct(current: number, max: number): number {
  if (max <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((current / max) * 100)));
}

// Multi-stop ramp matching the prior StatusBar fill colors so the bar
// shifts through green / yellow / orange / red as the vital depletes.
function colorForPct(value: number): string {
  if (value >= 80) return '#87a987';
  if (value >= 60) return '#e6c384';
  if (value >= 40) return '#d99a6c';
  if (value >= 20) return '#e46876';
  return '#7d1d1d';
}

// Dark, tinted variant of the fill color for the overlaid numbers.
// Multiplies the rgb channels so the text reads as the same hue family
// as its bar (green text on green fill, red text on red fill, etc.)
// but dark enough to stay legible against the bright fill.
function tintForFill(hex: string): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  const k = 0.25;
  return `rgb(${Math.round(r * k)}, ${Math.round(g * k)}, ${Math.round(b * k)})`;
}

// Bottom status bar. Lives under the input row. Carries the live
// vitals (hp / mana / move from GMCP Char.Vitals) on the left and a
// clock on the right. Each vital reads as: LABEL  PCT% [fill bar with
// current/max numbers overlaid].
export function StatusBar() {
  const [now, setNow] = useState(() => new Date());
  const [vitals, setVitals] = useState<Vitals | null>(null);

  useEffect(() => {
    const tick = () => setNow(new Date());
    tick();
    const id = window.setInterval(tick, 15_000);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let cancelled = false;

    onGmcp((payload) => {
      if (payload.package !== 'Char.Vitals') return;
      const data = payload.data ?? {};
      setVitals({
        hp: num(data.hp, 0),
        maxhp: num(data.maxhp, 0),
        mana: num(data.mana, 0),
        maxmana: num(data.maxmana, 0),
        move: num(data.move, 0),
        maxmove: num(data.maxmove, 0),
      });
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') setVitals(null);
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
      </div>
      <div className="statusbar-right">
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
