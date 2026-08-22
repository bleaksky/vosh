import { useEffect, useState } from 'react';
import { getTarget, onGmcpPackage, onState, onTarget, type QuickKey } from '../lib/session';
import { subscribeWellSplits, wellSplitsOpen } from '../lib/wellSplits';

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
// and 5-7 waxing. Four cardinal phases drawn as crisp 10px stroke
// circles (the Ember canvas treatment); intermediate phases collapse
// to the nearest neighbour, waxing vs waning preserved by which half
// fills. currentColor drives both stroke and fill so the active
// accent tint applies from CSS.
function MoonIcon({ phase }: { phase: number }) {
  const n = ((Math.round(phase) % 8) + 8) % 8;
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
      {n === 0 && <circle cx="5" cy="5" r="4" fill="currentColor" stroke="none" />}
      {n >= 1 && n <= 3 && (
        <>
          <circle cx="5" cy="5" r="4" fill="none" stroke="currentColor" strokeWidth="1" />
          <path d="M5 1a4 4 0 0 0 0 8z" fill="currentColor" stroke="none" />
        </>
      )}
      {n === 4 && <circle cx="5" cy="5" r="4" fill="none" stroke="currentColor" strokeWidth="1" />}
      {n >= 5 && (
        <>
          <circle cx="5" cy="5" r="4" fill="none" stroke="currentColor" strokeWidth="1" />
          <path d="M5 1a4 4 0 0 1 0 8z" fill="currentColor" stroke="none" />
        </>
      )}
    </svg>
  );
}

export function StatusBar() {
  const [now, setNow] = useState(() => new Date());
  const [splits, setSplits] = useState(() => wellSplitsOpen());
  useEffect(() => subscribeWellSplits(setSplits), []);
  const [userTarget, setUserTarget] = useState<string | null>(null);
  const [quickKeys, setQuickKeys] = useState<QuickKey[]>([]);
  const [moons, setMoons] = useState<MoonsState>({ moons: [] });

  useEffect(() => {
    const wallTick = () => setNow(new Date());
    wallTick();
    const id = window.setInterval(wallTick, 15_000);
    return () => window.clearInterval(id);
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
      {/* Center slot: the pane list while well splits are open, else
          an empty spacer keeping left and right anchored. */}
      <div className="statusbar-center">
        {splits && (
          <span className="statusbar-panes" aria-label="well panes">
            <span className="is-active">1 session</span>
            <span>2 chat</span>
            <span>3 log</span>
          </span>
        )}
      </div>
      <div className="statusbar-right">
        {moonsBlock}
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
          <MoonIcon phase={m.phase ?? -1} />
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
