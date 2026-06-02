import { useEffect, useState } from 'react';
import {
  getTarget,
  getUiConfig,
  onGmcpPackage,
  onState,
  onTarget,
  subscribeMoonsPositionChanged,
  type MoonsPosition,
  type QuickKey,
} from '../lib/session';
import { InlineMudTime, InlineTick } from './VitalsBar';
import { useTickState } from '../lib/useTickState';
import { formatMudTime, useWorldTime } from '../lib/useWorldTime';

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

function formatClock(date: Date): string {
  const hh = String(date.getHours()).padStart(2, '0');
  const mm = String(date.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

// Aabahran's World.Moons GMCP uses 0=full, 4=new, with 1-3 waning
// and 5-7 waxing. The four geometric circles cover the cardinal
// phases legibly; intermediate phases collapse to the nearest
// neighbour (waxing vs waning is preserved).
function moonGlyphFromIndex(phase: number): string {
  const n = ((Math.round(phase) % 8) + 8) % 8;
  if (n === 0) return '○'; // full
  if (n >= 1 && n <= 3) return '◑'; // waning (right side dark)
  if (n === 4) return '●'; // new
  return '◐'; // waxing (left side dark)
}

export function StatusBar({
  embedTick = false,
  embedTime = false,
}: { embedTick?: boolean; embedTime?: boolean } = {}) {
  const [now, setNow] = useState(() => new Date());
  const [userTarget, setUserTarget] = useState<string | null>(null);
  const [quickKeys, setQuickKeys] = useState<QuickKey[]>([]);
  const [moons, setMoons] = useState<MoonsState>({ moons: [] });
  const [moonsPosition, setMoonsPosition] = useState<MoonsPosition>('right-edge');

  useEffect(() => {
    const wallTick = () => setNow(new Date());
    wallTick();
    const id = window.setInterval(wallTick, 15_000);
    return () => window.clearInterval(id);
  }, []);

  // Seed the moons placement from the persisted UI config, then keep
  // it live by listening for the Settings window's broadcast on
  // every save. Falls back to "right-edge" (the historical position)
  // if the backend is unreachable.
  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void getUiConfig()
      .then((cfg) => {
        if (!cancelled) setMoonsPosition(cfg.moons_position);
      })
      .catch(() => undefined);
    void subscribeMoonsPositionChanged((value) => {
      if (!cancelled) setMoonsPosition(value);
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
    let unsubTarget: (() => void) | undefined;
    let cancelled = false;

    void getTarget()
      .then((snap) => {
        if (cancelled) return;
        setUserTarget(snap.name);
        setQuickKeys(snap.quick_keys);
      })
      .catch(() => undefined);

    onGmcpPackage<Partial<MoonsState>>('World.Moons', (data) => {
      if (!data || typeof data !== 'object') return;
      const next: MoonsState = {
        moons: Array.isArray(data.moons) ? data.moons : [],
      };
      if (data.eclipse !== undefined) next.eclipse = data.eclipse;
      if (data.triad !== undefined) next.triad = data.triad;
      if (data.near_alignment !== undefined) next.near_alignment = data.near_alignment;
      setMoons(next);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onTarget((payload) => {
      setUserTarget(payload.name);
      setQuickKeys(payload.quick_keys);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubTarget = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setUserTarget(null);
        setQuickKeys([]);
        setMoons({ moons: [] });
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

  const configuredKeys = quickKeys.filter((q) => q.verb.length > 0);
  const moonsBlock = moons.moons.length > 0 ? <MoonsBlock moons={moons} /> : null;

  return (
    <div className="statusbar">
      <div className="statusbar-left">
        {(userTarget || configuredKeys.length > 0) && (
          <span className="statusbar-target">
            {userTarget && (
              <>
                <span className="statusbar-target-label">tar</span>
                <span className="statusbar-target-value">{userTarget}</span>
              </>
            )}
            {configuredKeys.length > 0 && (
              <ul className="statusbar-qkeys" aria-label="quick keys">
                {configuredKeys.map((qk, i) => (
                  <li key={qk.name} className="statusbar-qkey">
                    {i > 0 && (
                      <span className="statusbar-qkey-sep" aria-hidden="true">
                        ·
                      </span>
                    )}
                    <span className="statusbar-qkey-name">{qk.name}</span>
                    <span className="statusbar-qkey-verb">{qk.verb}</span>
                  </li>
                ))}
              </ul>
            )}
          </span>
        )}
      </div>
      {/* Centered, always-visible tick + MUD time chip. Bolder than
          surrounding chrome so the eye lands on it immediately —
          tick is time-critical (regen, combat timing) and MUD time
          sets daily context (shop hours, day/night). Renders nothing
          for whichever source is inactive, so a server that doesn't
          ship World.Time just shows the tick countdown alone, and
          vice versa. The moons block docks here when the user picks
          `before-time` / `after-time` so the phases sit next to the
          MUD time chip they relate to. */}
      <div className="statusbar-center">
        {moonsPosition === 'before-time' && moonsBlock}
        <StatusTimeChip />
        {moonsPosition === 'after-time' && moonsBlock}
      </div>
      <div className="statusbar-right">
        {embedTick && <InlineTick className="statusbar-tick" />}
        {/* embedTime renders before the wall-clock at the bottom of the
            block so the MUD time chip (when embedded) reads to the
            left of the always-on wall-clock. The wall-clock now
            unconditionally shows the local system time per user ask. */}
        {moonsPosition === 'right-edge' && moonsBlock}
        {embedTime && <InlineMudTime className="statusbar-time" />}
        <span className="statusbar-clock" title="local wall-clock time">
          {formatClock(now)}
        </span>
      </div>
    </div>
  );
}

function MoonsBlock({ moons }: { moons: MoonsState }) {
  return (
    <span className="statusbar-moons">
      {moons.moons.map((m, i) => (
        <span
          key={m.name ?? i}
          className={`statusbar-moon${m.active ? ' is-active' : ''}`}
          data-tooltip={
            `${m.name ?? 'moon'}: ${m.phase_name ?? `phase ${m.phase ?? '?'}`}` +
            (m.active ? ' (active)' : '')
          }
        >
          {moonGlyphFromIndex(m.phase ?? -1)}
        </span>
      ))}
      {moons.eclipse && <span className="statusbar-moon-badge is-eclipse">eclipse</span>}
      {!moons.eclipse && moons.triad && (
        <span className="statusbar-moon-badge is-triad">triad</span>
      )}
      {!moons.eclipse && !moons.triad && moons.near_alignment && (
        <span className="statusbar-moon-badge is-near">near</span>
      )}
    </span>
  );
}

// Always-visible tick + MUD time chip rendered in the center of the
// status bar. Both halves use the shared `InlineTick` / `InlineMudTime`
// which delegate their rendering to `ChipFrame`, so the user-picked
// chip style (value-only / caption + value / icon + value) applies
// here too. Each half renders independently and null-returns when
// inactive; when both are null the whole slot collapses.
function StatusTimeChip() {
  const { active } = useTickState();
  const worldTime = useWorldTime();
  const mudTime = formatMudTime(worldTime);
  if (!active && !mudTime) return null;
  return (
    <div className="statusbar-timechip" aria-label="tick and MUD time">
      <InlineTick className="statusbar-timechip-block" />
      {active && mudTime && <span className="statusbar-timechip-sep">·</span>}
      <InlineMudTime className="statusbar-timechip-block statusbar-timechip-time" />
    </div>
  );
}
