import { useEffect, useState } from 'react';
import { onGmcp, onState } from '../lib/session';
import type { ConnectionStatus } from './Connect';

interface Props {
  status: ConnectionStatus;
}

interface Vitals {
  hp: number;
  maxhp: number;
  sp: number;
  maxsp: number;
  mv: number;
  maxmv: number;
}

function formatClock(date: Date): string {
  const hh = String(date.getHours()).padStart(2, '0');
  const mm = String(date.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

function describeStatus(status: ConnectionStatus): { label: string; tone: string } {
  switch (status.kind) {
    case 'idle':
      return { label: 'idle', tone: 'muted' };
    case 'connecting':
      return { label: `connecting ${status.host}:${status.port}`, tone: 'warn' };
    case 'connected':
      return { label: `${status.host}:${status.port}`, tone: 'ok' };
    case 'error':
      return { label: `error ${status.message}`, tone: 'err' };
  }
}

// Pick a tone for a vital based on its current ratio. Mirrors common
// MUD HUD conventions: green above half, yellow in the warning band,
// red when critical.
function vitalTone(current: number, max: number): string {
  if (max <= 0) return 'muted';
  const ratio = current / max;
  if (ratio >= 0.5) return 'ok';
  if (ratio >= 0.25) return 'warn';
  return 'err';
}

// Pull a numeric field from a GMCP Char.Vitals payload. Aabahran (and
// most ROM-derived MUDs) sometimes serialize these as strings, so
// coerce defensively.
function num(value: unknown, fallback: number): number {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string') {
    const n = Number(value);
    if (Number.isFinite(n)) return n;
  }
  return fallback;
}

// tmux-style single-row status line. Left side carries the brand and
// the connection state; middle holds live vitals from GMCP
// Char.Vitals; right side holds the clock.
export function StatusBar({ status }: Props) {
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
        sp: num(data.sp, 0),
        maxsp: num(data.maxsp, 0),
        mv: num(data.mv, 0),
        maxmv: num(data.maxmv, 0),
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

  const s = describeStatus(status);

  return (
    <div className="statusbar">
      <div className="statusbar-left">
        <span className="brand-block">[vosh]</span>
        <span className={`statusbar-seg statusbar-tone-${s.tone}`}>{s.label}</span>
      </div>
      <div className="statusbar-center">
        {vitals && (
          <>
            <VitalSeg label="hp" cur={vitals.hp} max={vitals.maxhp} />
            <VitalSeg label="sp" cur={vitals.sp} max={vitals.maxsp} />
            <VitalSeg label="mv" cur={vitals.mv} max={vitals.maxmv} />
          </>
        )}
      </div>
      <div className="statusbar-right">
        <span className="statusbar-seg statusbar-clock">{formatClock(now)}</span>
      </div>
    </div>
  );
}

function VitalSeg({ label, cur, max }: { label: string; cur: number; max: number }) {
  const tone = vitalTone(cur, max);
  return (
    <span className={`statusbar-vital statusbar-tone-${tone}`}>
      <span className="statusbar-vital-label">{label}</span>
      <span className="statusbar-vital-value">
        {cur}
        <span className="statusbar-vital-sep">/</span>
        {max}
      </span>
    </span>
  );
}
