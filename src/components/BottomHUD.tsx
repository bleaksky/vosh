import { useEffect, useRef, useState } from 'react';
import {
  getTarget,
  getUiConfig,
  onGmcp,
  onState,
  onTarget,
  onTick,
  subscribeTrackedAffectsChanged,
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
  // Tintin-style tick display: count up real seconds since the last
  // World.Time hour change. Resets only on hour change, not on the
  // backend's local-interval auto-fire (which can fire ahead of or
  // behind the MUD's actual tick). Matches ~/tintin/prompt.tin's
  // ui_tick_secs behavior.
  const [tickSecs, setTickSecs] = useState(0);
  const tickResetAtRef = useRef<number>(Date.now());
  const prevHourRef = useRef<number | string | null>(null);
  const [combat, setCombat] = useState<CombatState | null>(null);
  const [userTarget, setUserTarget] = useState<string | null>(null);
  const [quickKeys, setQuickKeys] = useState<QuickKey[]>([]);
  const [affects, setAffects] = useState<Affect[]>([]);
  const [tracked, setTracked] = useState<string[]>([]);

  useEffect(() => {
    let cancelled = false;
    let unsubTauri: (() => void) | undefined;
    getUiConfig()
      .then((cfg) => {
        if (!cancelled) setTracked(cfg.tracked_affects ?? []);
      })
      .catch(() => {});
    // Local window event for in-window edits (chromeless settings).
    const handler = (event: Event) => {
      const detail = (event as CustomEvent<string[]>).detail;
      if (Array.isArray(detail)) setTracked(detail);
    };
    window.addEventListener('vosh:tracked-affects-changed', handler as EventListener);
    // Cross-window Tauri event for the standalone settings window.
    // Without this, the BottomHUD doesn't see tracked-affect edits
    // until the next launch.
    subscribeTrackedAffectsChanged((list) => {
      if (!cancelled) setTracked(list);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubTauri = fn;
    });
    return () => {
      cancelled = true;
      window.removeEventListener(
        'vosh:tracked-affects-changed',
        handler as EventListener,
      );
      unsubTauri?.();
    };
  }, []);

  // Real-time local counter for the tick display. Mirrors tintin's
  // 1-second ticker: every wall-clock second, recompute elapsed
  // seconds since the last hour-change reset. setInterval runs at
  // 250ms to keep the display responsive even if the browser
  // throttles longer timers under tab inactivity.
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
        const incoming = payload.data as WorldTime;
        setWorld((prev) => ({ ...prev, ...incoming }));
        // Reset the tintin-style tick counter on every hour change.
        // The first World.Time after connect just records the hour,
        // subsequent ones with a different hour are the tick fires.
        const hour = incoming.hour;
        if (hour !== undefined && hour !== null) {
          if (prevHourRef.current !== null && prevHourRef.current !== hour) {
            tickResetAtRef.current = Date.now();
            setTickSecs(0);
          }
          prevHourRef.current = hour;
        }
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
        // Forget the last hour so the next connection's first
        // World.Time push is treated as the seed, not a hour-change.
        prevHourRef.current = null;
        tickResetAtRef.current = Date.now();
        setTickSecs(0);
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
      // Tick beep is intentionally disabled. The backend fires on a
      // local-clock interval (default 30s) that drifts from the
      // MUD's actual tick — the only canonical signal is the
      // World.Time hour change, which the display already tracks.
      // Beeping on the local-fire produced false alarms ahead of
      // (or behind) every real tick.
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
  // Tintin-style display: tickSecs is incremented locally every
  // second and reset only on World.Time hour change. Show the pill
  // whenever the tick state is enabled — the actual counter doesn't
  // depend on the backend's interval/remaining math anymore.
  const tickDisplay = tick?.enabled ? tickSecs : null;
  // Flash when we're approaching the expected tick interval. We
  // don't know the MUD's real cadence here, so use the configured
  // interval as the upper bound and warn during the last 5s.
  const intervalSecs =
    tick?.enabled && tick.interval_ms !== undefined
      ? Math.max(1, Math.floor(tick.interval_ms / 1000))
      : null;
  const tickFlash =
    intervalSecs !== null && tickDisplay !== null && tickDisplay >= intervalSecs - 5;

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
          {tickDisplay !== null && (
            <span className={`bhud-world-tick${tickFlash ? ' is-flashing' : ''}`}>
              {tickDisplay}s
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
