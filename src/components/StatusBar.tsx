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
}

interface MoonInfo {
  name?: string;
  active?: boolean;
  phase?: number;
  phase_name?: string;
}

interface MoonsState {
  moons: MoonInfo[];
  eclipse?: boolean;
  triad?: boolean;
  near_alignment?: boolean;
}

// Eight distinct moon phase emoji so each phase reads at a glance.
// Aabahran's GMCP convention (matching the user's TinTin file) is
// 0 = full, 4 = new, with phases 1-3 waning and 5-7 waxing.
const MOON_GLYPHS: Record<number, string> = {
  0: '🌕',
  1: '🌖',
  2: '🌗',
  3: '🌘',
  4: '🌑',
  5: '🌒',
  6: '🌓',
  7: '🌔',
};

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

// Green at full, maroon at empty. The same ramp drives the fill block
// and the percent/label text so the eye reads severity at a glance.
// Tones lean into the Kanso Zen palette so the bars match the rest of
// the chrome.
function colorForPct(value: number): string {
  if (value >= 80) return '#87a987'; // kanso bright green
  if (value >= 60) return '#e6c384'; // kanso bright yellow
  if (value >= 40) return '#d99a6c'; // warm orange
  if (value >= 20) return '#e46876'; // kanso bright red
  return '#7d1d1d'; // deep maroon
}

function formatHour(value: number | string | undefined): string | null {
  const n = num(value);
  if (n === null) return null;
  const hour = ((Math.trunc(n) % 24) + 24) % 24;
  // No colon — military-style HH00 reads tighter and matches the
  // "less eye travel" goal across the bottom rail.
  return `${hour.toString().padStart(2, '0')}00`;
}

function StatBar({
  label,
  current,
  max,
  delta,
}: {
  label: string;
  current: number | string | undefined;
  max: number | string | undefined;
  delta: number | null;
}) {
  const value = pct(current, max);
  const fillColor = colorForPct(value);
  const currentNum = num(current);
  const maxNum = num(max);
  const showDelta = delta !== null && delta !== 0;
  const deltaColor = delta !== null && delta > 0 ? '#87a987' : '#e46876';
  const arrow = delta !== null && delta > 0 ? '↑' : '↓';
  return (
    <div className="statusbar-bar">
      <span className="statusbar-bar-label">{label}</span>
      <span className="statusbar-bar-percent">{value}%</span>
      <span className="statusbar-bar-track">
        <span
          className="statusbar-bar-fill"
          style={{ width: `${value}%`, background: fillColor }}
          aria-hidden="true"
        />
        <span className="statusbar-bar-text">
          {currentNum ?? current ?? '-'}/{maxNum ?? max ?? '-'}
        </span>
      </span>
      {showDelta && (
        <span className="statusbar-bar-delta" style={{ color: deltaColor }}>
          <span aria-hidden="true">{arrow}</span>
          {delta! > 0 ? `+${delta}` : delta}
        </span>
      )}
    </div>
  );
}

interface TickDeltas {
  hp: number | null;
  mn: number | null;
  mv: number | null;
}

const NO_DELTAS: TickDeltas = { hp: null, mn: null, mv: null };

interface VitalsSnapshot {
  hp: number | null;
  mn: number | null;
  mv: number | null;
}

function snapshotVitals(v: Vitals): VitalsSnapshot {
  return {
    hp: num(v.hp),
    mn: num(pickFirst(v.mp, v.mana)),
    mv: num(pickFirst(v.sp, v.move, v.movement)),
  };
}

function diffSnapshots(prev: VitalsSnapshot, curr: VitalsSnapshot): TickDeltas {
  return {
    hp: prev.hp !== null && curr.hp !== null ? curr.hp - prev.hp : null,
    mn: prev.mn !== null && curr.mn !== null ? curr.mn - prev.mn : null,
    mv: prev.mv !== null && curr.mv !== null ? curr.mv - prev.mv : null,
  };
}

export function StatusBar() {
  const [vitals, setVitals] = useState<Vitals>({});
  const [world, setWorld] = useState<WorldTime>({});
  const [moons, setMoons] = useState<MoonsState>({ moons: [] });
  const [tick, setTick] = useState<TickPayload | null>(null);
  const [deltas, setDeltas] = useState<TickDeltas>(NO_DELTAS);
  const lastFiredRef = useRef<number>(0);
  const prevVitalsSnapshotRef = useRef<VitalsSnapshot | null>(null);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let unsubTick: (() => void) | undefined;
    let cancelled = false;

    onGmcp((payload) => {
      if (payload.package === 'Char.Vitals') {
        const next = (payload.data ?? {}) as Vitals;
        const snapshot = snapshotVitals(next);
        const prev = prevVitalsSnapshotRef.current;
        if (prev) {
          setDeltas(diffSnapshots(prev, snapshot));
        }
        prevVitalsSnapshotRef.current = snapshot;
        setVitals(next);
        return;
      }
      if (payload.package === 'World.Time' && payload.data && typeof payload.data === 'object') {
        setWorld((prev) => ({ ...prev, ...(payload.data as WorldTime) }));
        return;
      }
      if (payload.package === 'World.Moons' && payload.data && typeof payload.data === 'object') {
        const data = payload.data as Partial<MoonsState>;
        const next: MoonsState = {
          moons: Array.isArray(data.moons) ? data.moons : [],
        };
        if (data.eclipse !== undefined) next.eclipse = data.eclipse;
        if (data.triad !== undefined) next.triad = data.triad;
        if (data.near_alignment !== undefined) next.near_alignment = data.near_alignment;
        setMoons(next);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setVitals({});
        setWorld({});
        setMoons({ moons: [] });
        setTick(null);
        setDeltas(NO_DELTAS);
        prevVitalsSnapshotRef.current = null;
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
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
      if (cancelled) fn();
      else unsubTick = fn;
    });

    return () => {
      cancelled = true;
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
      <StatBar label="HP" current={vitals.hp} max={vitals.maxhp} delta={deltas.hp} />
      <StatBar
        label="MN"
        current={pickFirst(vitals.mp, vitals.mana)}
        max={pickFirst(vitals.maxmp, vitals.maxmana)}
        delta={deltas.mn}
      />
      <StatBar
        label="MV"
        current={pickFirst(vitals.sp, vitals.move, vitals.movement)}
        max={pickFirst(vitals.maxsp, vitals.maxmove, vitals.maxmovement)}
        delta={deltas.mv}
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
      {moons.moons.length > 0 && (
        <div
          className="statusbar-cell statusbar-moons"
          title={moons.moons
            .map(
              (m) =>
                `${m.name ?? '?'}: ${m.phase_name ?? `phase ${m.phase ?? '?'}`}` +
              (m.active ? ' (active)' : ''),
            )
            .join('\n')}
        >
          {moons.moons.map((m, i) => (
            <span
              key={m.name ?? i}
              className={`moon-glyph${m.active ? ' moon-active' : ''}`}
            >
              {MOON_GLYPHS[m.phase ?? -1] ?? '◯'}
            </span>
          ))}
          {moons.eclipse && <span className="moon-badge moon-eclipse">eclipse</span>}
          {!moons.eclipse && moons.triad && (
            <span className="moon-badge moon-triad">triad</span>
          )}
          {!moons.eclipse && !moons.triad && moons.near_alignment && (
            <span className="moon-badge moon-near">near</span>
          )}
        </div>
      )}
    </div>
  );
}
