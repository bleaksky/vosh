import { useEffect, useRef, useState } from 'react';
import { onGmcp, onState, onTick, type TickPayload } from '../lib/session';

function playBeep() {
  try {
    type WindowWithWebkit = Window & { webkitAudioContext?: typeof AudioContext };
    const w = window as WindowWithWebkit;
    const Ctx = window.AudioContext ?? w.webkitAudioContext;
    if (!Ctx) return;
    const ctx = new Ctx();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.frequency.value = 880;
    osc.type = 'sine';
    osc.connect(gain);
    gain.connect(ctx.destination);
    const now = ctx.currentTime;
    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.exponentialRampToValueAtTime(0.18, now + 0.01);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.18);
    osc.start(now);
    osc.stop(now + 0.2);
    osc.onended = () => ctx.close();
  } catch {
    // Audio unavailable; ignore.
  }
}

interface Vitals {
  hp?: number | string;
  maxhp?: number | string;
  mp?: number | string;
  maxmp?: number | string;
  mana?: number | string;
  maxmana?: number | string;
  sp?: number | string;
  maxsp?: number | string;
  move?: number | string;
  maxmove?: number | string;
  movement?: number | string;
  maxmovement?: number | string;
}

interface WorldTime {
  hour?: number | string;
  minute?: number | string;
  day?: number | string;
  month?: string;
  year?: number | string;
  sky?: string;
  weather?: string;
  precip?: string;
  light?: string;
  moon?: string;
}

function pickFirst<T>(...values: (T | undefined)[]): T | undefined {
  for (const v of values) {
    if (v !== undefined && v !== null && v !== '') return v;
  }
  return undefined;
}

function num(value: number | string | undefined): number | null {
  if (value === undefined) return null;
  const n = typeof value === 'number' ? value : Number(value);
  return Number.isFinite(n) ? n : null;
}

function pct(current: number | string | undefined, max: number | string | undefined): number {
  const c = num(current);
  const m = num(max);
  if (c === null || m === null || m <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((c / m) * 100)));
}

function formatHour(value: number | string | undefined): string | null {
  const n = num(value);
  if (n === null) return null;
  const hour = ((Math.trunc(n) % 24) + 24) % 24;
  return `${hour.toString().padStart(2, '0')}:00`;
}

function StatBar({
  label,
  current,
  max,
  color,
}: {
  label: string;
  current: number | string | undefined;
  max: number | string | undefined;
  color: string;
}) {
  const value = pct(current, max);
  return (
    <div className="statusbar-bar">
      <span className="statusbar-bar-label">{label}</span>
      <span className="statusbar-bar-percent">{value}%</span>
      <span className="statusbar-bar-track" aria-hidden="true">
        <span
          className="statusbar-bar-fill"
          style={{ width: `${value}%`, background: color }}
        />
      </span>
      <span className="statusbar-bar-value">
        {current ?? '-'}/{max ?? '-'}
      </span>
    </div>
  );
}

export function StatusBar() {
  const [vitals, setVitals] = useState<Vitals>({});
  const [world, setWorld] = useState<WorldTime>({});
  const [tick, setTick] = useState<TickPayload | null>(null);
  const lastFiredRef = useRef<number>(0);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let unsubTick: (() => void) | undefined;

    onGmcp((payload) => {
      if (payload.package === 'Char.Vitals') {
        setVitals(payload.data ?? {});
        return;
      }
      if (payload.package === 'World.Time' && payload.data && typeof payload.data === 'object') {
        setWorld((prev) => ({ ...prev, ...(payload.data as WorldTime) }));
      }
    }).then((fn) => {
      unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setVitals({});
        setWorld({});
        setTick(null);
      }
    }).then((fn) => {
      unsubState = fn;
    });

    onTick((payload) => {
      setTick(payload);
      if (payload.fired && payload.sound) {
        const now = Date.now();
        if (now - lastFiredRef.current > 500) {
          lastFiredRef.current = now;
          playBeep();
        }
      }
    }).then((fn) => {
      unsubTick = fn;
    });

    return () => {
      unsubGmcp?.();
      unsubState?.();
      unsubTick?.();
    };
  }, []);

  const tickElapsed =
    tick?.enabled !== undefined && tick.enabled
      ? Math.max(1, Math.ceil((tick.interval_ms - tick.remaining_ms) / 1000))
      : null;
  const tickFlash = tick?.enabled && tick.remaining_ms <= 5000;

  const hourLabel = formatHour(world.hour);
  const skyLabel = pickFirst(world.sky, world.light) as string | undefined;
  const weatherLabel = pickFirst(world.weather, world.precip) as string | undefined;

  return (
    <div className="statusbar" role="status" aria-label="vitals and world conditions">
      <StatBar label="HP" current={vitals.hp} max={vitals.maxhp} color="#3fb950" />
      <StatBar
        label="MN"
        current={pickFirst(vitals.mp, vitals.mana)}
        max={pickFirst(vitals.maxmp, vitals.maxmana)}
        color="#1f6feb"
      />
      <StatBar
        label="MV"
        current={pickFirst(vitals.sp, vitals.move, vitals.movement)}
        max={pickFirst(vitals.maxsp, vitals.maxmove, vitals.maxmovement)}
        color="#d29922"
      />
      <div className="statusbar-divider" aria-hidden="true" />
      <div className={`statusbar-cell statusbar-tick${tickFlash ? ' statusbar-flash' : ''}`}>
        <span className="statusbar-key">tick</span>
        <span className="statusbar-value">{tickElapsed === null ? 'off' : `${tickElapsed}s`}</span>
      </div>
      {hourLabel && (
        <div className="statusbar-cell">
          <span className="statusbar-value">{hourLabel}</span>
        </div>
      )}
      {(skyLabel || weatherLabel) && (
        <div className="statusbar-cell statusbar-conditions">
          {skyLabel && <span className="statusbar-value">{skyLabel}</span>}
          {skyLabel && weatherLabel && <span className="statusbar-sep">·</span>}
          {weatherLabel && <span className="statusbar-value">{weatherLabel}</span>}
        </div>
      )}
      {world.moon && (
        <div className="statusbar-cell">
          <span className="statusbar-key">moon</span>
          <span className="statusbar-value">{world.moon}</span>
        </div>
      )}
    </div>
  );
}
