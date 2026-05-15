import { useEffect, useRef, useState } from 'react';
import {
  getTarget,
  getUiConfig,
  onGmcp,
  onState,
  onTarget,
  onTick,
  type QuickKey,
  type TickPayload,
} from '../lib/session';
import {
  colorForDuration,
  formatDuration,
  normalizeAffectName,
  type Affect,
} from '../lib/affects';
import { Input, type InputHandle } from './Input';
import type { Ref } from 'react';

interface Props {
  inputRef: Ref<InputHandle>;
  enabled: boolean;
  onError: (message: string) => void;
  onLocalEcho: (text: string) => void;
  onRequestDrawer: () => void;
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

interface CombatState {
  name: string;
  hp?: number;
  condition?: string;
}

const MOON_GLYPHS: Record<number, string> = {
  0: '🌕', 1: '🌖', 2: '🌗', 3: '🌘', 4: '🌑', 5: '🌒', 6: '🌓', 7: '🌔',
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

function colorForPct(value: number): string {
  if (value >= 80) return '#87a987';
  if (value >= 60) return '#e6c384';
  if (value >= 40) return '#d99a6c';
  if (value >= 20) return '#e46876';
  return '#7d1d1d';
}

function formatHour(value: number | string | undefined): string | null {
  const n = num(value);
  if (n === null) return null;
  const hour = ((Math.trunc(n) % 24) + 24) % 24;
  return `${hour.toString().padStart(2, '0')}00`;
}

function asNumber(value: unknown): number | undefined {
  if (typeof value === 'number') return Number.isFinite(value) ? value : undefined;
  if (typeof value === 'string') {
    const n = Number(value);
    return Number.isFinite(n) ? n : undefined;
  }
  return undefined;
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

// Compact mini-meter: label + colored fill + percentage. Replaces the
// tall StatBar from the old bottom-rail StatusBar so HP/MN/MV fit
// inline next to the combat chip and affect pills.
function MiniVital({
  label,
  current,
  max,
}: {
  label: string;
  current: number | string | undefined;
  max: number | string | undefined;
}) {
  const value = pct(current, max);
  const fill = colorForPct(value);
  const c = num(current);
  const m = num(max);
  return (
    <div className="bhud-vital" title={`${label} ${c ?? '-'}/${m ?? '-'}`}>
      <span className="bhud-vital-label">{label}</span>
      <span className="bhud-vital-track">
        <span
          className="bhud-vital-fill"
          style={{ width: `${value}%`, background: fill }}
          aria-hidden="true"
        />
        <span className="bhud-vital-text">{value}%</span>
      </span>
    </div>
  );
}

export function BottomHUD({
  inputRef,
  enabled,
  onError,
  onLocalEcho,
  onRequestDrawer,
}: Props) {
  const [vitals, setVitals] = useState<Vitals>({});
  const [world, setWorld] = useState<WorldTime>({});
  const [moons, setMoons] = useState<MoonsState>({ moons: [] });
  const [tick, setTick] = useState<TickPayload | null>(null);
  const [combat, setCombat] = useState<CombatState | null>(null);
  const [userTarget, setUserTarget] = useState<string | null>(null);
  const [quickKeys, setQuickKeys] = useState<QuickKey[]>([]);
  const [affects, setAffects] = useState<Affect[]>([]);
  const [tracked, setTracked] = useState<string[]>([]);
  const lastFiredRef = useRef<number>(0);

  useEffect(() => {
    let cancelled = false;
    getUiConfig()
      .then((cfg) => {
        if (!cancelled) setTracked(cfg.tracked_affects ?? []);
      })
      .catch(() => {});
    const handler = (event: Event) => {
      const detail = (event as CustomEvent<string[]>).detail;
      if (Array.isArray(detail)) setTracked(detail);
    };
    window.addEventListener('mudclient:tracked-affects-changed', handler as EventListener);
    return () => {
      cancelled = true;
      window.removeEventListener(
        'mudclient:tracked-affects-changed',
        handler as EventListener,
      );
    };
  }, []);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let unsubTick: (() => void) | undefined;
    let unsubTarget: (() => void) | undefined;
    let cancelled = false;

    // Seed target state since the backend retains quick-keys across
    // sessions but only pushes target events on change.
    getTarget()
      .then((snap) => {
        if (cancelled) return;
        setUserTarget(snap.name);
        setQuickKeys(snap.quick_keys);
      })
      .catch(() => {});

    onGmcp((payload) => {
      const pkg = payload.package;
      if (pkg === 'Char.Vitals') {
        setVitals((payload.data ?? {}) as Vitals);
        return;
      }
      if (pkg === 'World.Time' && payload.data && typeof payload.data === 'object') {
        setWorld((prev) => ({ ...prev, ...(payload.data as WorldTime) }));
        return;
      }
      if (pkg === 'World.Moons' && payload.data && typeof payload.data === 'object') {
        const data = payload.data as Partial<MoonsState>;
        const next: MoonsState = {
          moons: Array.isArray(data.moons) ? data.moons : [],
        };
        if (data.eclipse !== undefined) next.eclipse = data.eclipse;
        if (data.triad !== undefined) next.triad = data.triad;
        if (data.near_alignment !== undefined) next.near_alignment = data.near_alignment;
        setMoons(next);
        return;
      }
      if (pkg === 'Char.Combat') {
        setCombat(extractCombat(payload.data));
        return;
      }
      if (pkg === 'Char.Affects' && payload.data && typeof payload.data === 'object') {
        const data = payload.data as { affects?: Affect[] };
        const list = Array.isArray(data.affects) ? data.affects : [];
        const seen = new Set<string>();
        const deduped: Affect[] = [];
        for (const a of list) {
          if (!a?.name || seen.has(a.name)) continue;
          seen.add(a.name);
          deduped.push(a);
        }
        setAffects(deduped);
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
        setCombat(null);
        setUserTarget(null);
        setAffects([]);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
    });

    onTarget((payload) => {
      setUserTarget(payload.name);
      setQuickKeys(payload.quick_keys);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubTarget = fn;
    });

    onTick((payload) => {
      setTick(payload);
      if (payload.fired && payload.sound) {
        const now = Date.now();
        if (now - lastFiredRef.current > 500) {
          lastFiredRef.current = now;
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
            const t = ctx.currentTime;
            gain.gain.setValueAtTime(0.0001, t);
            gain.gain.exponentialRampToValueAtTime(0.18, t + 0.01);
            gain.gain.exponentialRampToValueAtTime(0.0001, t + 0.18);
            osc.start(t);
            osc.stop(t + 0.2);
            osc.onended = () => ctx.close();
          } catch {
            // ignore
          }
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
      unsubTarget?.();
    };
  }, []);

  const hourLabel = formatHour(world.hour);
  const skyLabel = pickFirst(world.sky, world.light);
  const weatherLabel = pickFirst(world.weather, world.precip);
  const tickRemaining =
    tick?.enabled && tick.remaining_ms !== undefined
      ? Math.max(0, Math.ceil(tick.remaining_ms / 1000))
      : null;
  const tickFlash = tick?.enabled && tick.remaining_ms <= 5000;

  const liveByKey = new Map<string, Affect>();
  for (const a of affects) liveByKey.set(normalizeAffectName(a.name), a);
  const configuredKeys = quickKeys.filter((q) => q.verb.length > 0);

  const hasAnyData =
    vitals.hp !== undefined ||
    vitals.mp !== undefined ||
    vitals.mana !== undefined ||
    !!combat ||
    !!userTarget ||
    affects.length > 0 ||
    tracked.length > 0;

  return (
    <div className="bottom-hud" aria-label="bottom hud">
      <div className="bhud-input-row">
        <Input
          ref={inputRef}
          enabled={enabled}
          onError={onError}
          onLocalEcho={onLocalEcho}
        />
        <div className="bhud-world" aria-label="world conditions">
          {tickRemaining !== null && (
            <span className={`bhud-world-tick${tickFlash ? ' is-flashing' : ''}`}>
              {tickRemaining}s
            </span>
          )}
          {hourLabel && <span className="bhud-world-hour">{hourLabel}</span>}
          {(skyLabel || weatherLabel) && (
            <span
              className="bhud-world-sky"
              title={[skyLabel, weatherLabel].filter(Boolean).join(' · ')}
            >
              {weatherLabel ?? skyLabel}
            </span>
          )}
          {moons.moons.length > 0 && (
            <span
              className="bhud-world-moons"
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
                  className={`bhud-moon${m.active ? ' is-active' : ''}`}
                >
                  {MOON_GLYPHS[m.phase ?? -1] ?? '◯'}
                </span>
              ))}
              {moons.eclipse && <span className="bhud-moon-badge is-eclipse">eclipse</span>}
              {!moons.eclipse && moons.triad && (
                <span className="bhud-moon-badge is-triad">triad</span>
              )}
            </span>
          )}
          <button
            type="button"
            className="bhud-drawer-button"
            onClick={onRequestDrawer}
            title="open drawer (F2): chat, wealth, group, affects"
            aria-label="open auxiliary drawer"
          >
            ⋯
          </button>
        </div>
      </div>
      <div className={`bhud-stats-row${hasAnyData ? '' : ' is-empty'}`}>
        <div className="bhud-vitals">
          <MiniVital label="HP" current={vitals.hp} max={vitals.maxhp} />
          <MiniVital
            label="MN"
            current={pickFirst(vitals.mp, vitals.mana)}
            max={pickFirst(vitals.maxmp, vitals.maxmana)}
          />
          <MiniVital
            label="MV"
            current={pickFirst(vitals.sp, vitals.move, vitals.movement)}
            max={pickFirst(vitals.maxsp, vitals.maxmove, vitals.maxmovement)}
          />
        </div>
        {combat && (
          <div
            className="bhud-combat"
            title={combat.condition ?? ''}
          >
            <span className="bhud-combat-swords" aria-hidden="true">⚔</span>
            <span className="bhud-combat-name">{combat.name}</span>
            {combat.hp !== undefined && (
              <span
                className="bhud-combat-hp"
                style={{ color: colorForPct(combat.hp) }}
              >
                {combat.hp}%
              </span>
            )}
          </div>
        )}
        {(userTarget || configuredKeys.length > 0) && (
          <div className="bhud-target">
            <span className="bhud-sep" aria-hidden="true">|</span>
            {userTarget && (
              <>
                <span className="bhud-target-label">tar</span>
                <span className="bhud-target-name">{userTarget}</span>
              </>
            )}
            {configuredKeys.length > 0 && (
              <ul className="bhud-qkeys" aria-label="quick keys">
                {configuredKeys.map((qk, i) => (
                  <li key={qk.name} className="bhud-qkey">
                    {i > 0 && <span className="bhud-qkey-sep" aria-hidden="true">·</span>}
                    <span className="bhud-qkey-name">{qk.name}</span>
                    <span className="bhud-qkey-verb">{qk.verb}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
        {tracked.length > 0 && (
          <div className="bhud-affects" aria-label="tracked affects">
            {tracked.map((wanted) => {
              const live = liveByKey.get(normalizeAffectName(wanted));
              if (!live) {
                return (
                  <span
                    key={wanted}
                    className="bhud-affect is-missing"
                    title={`${wanted} is not active`}
                  >
                    {wanted}
                  </span>
                );
              }
              const dur =
                typeof live.duration === 'number'
                  ? live.duration
                  : live.duration !== undefined
                    ? Number(live.duration)
                    : undefined;
              const color = colorForDuration(dur);
              return (
                <span
                  key={wanted}
                  className="bhud-affect"
                  style={{ borderColor: color, color }}
                  title={`${live.name} ${formatDuration(dur)}`}
                >
                  <span className="bhud-affect-name">{live.name}</span>
                  <span className="bhud-affect-dur">{formatDuration(dur)}</span>
                </span>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
