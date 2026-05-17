import { useEffect, useState } from 'react';
import { getTarget, onGmcp, onState, onTarget, type QuickKey } from '../lib/session';

interface CombatState {
  name: string;
  hp?: number;
  condition?: string;
}

function asNumber(value: unknown): number | undefined {
  if (typeof value === 'number') return Number.isFinite(value) ? value : undefined;
  if (typeof value === 'string') {
    const n = Number(value);
    return Number.isFinite(n) ? n : undefined;
  }
  return undefined;
}

// Aabahran's Char.Combat payload looks like:
//   { target: "The Baron Helgardium", hp_pct: 91, condition: "a few scratches" }
// An empty object signals the target is gone.
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

// Match the StatusBar percent ramp so HP visuals across the bottom rail
// share one color vocabulary.
function colorForPercent(pct: number): string {
  if (pct >= 80) return '#87a987';
  if (pct >= 60) return '#e6c384';
  if (pct >= 40) return '#d99a6c';
  if (pct >= 20) return '#e46876';
  return '#7d1d1d';
}

export function TargetBar() {
  // Left side: server-driven Char.Combat (who you're currently
  // fighting, with HP and condition).
  // Right side: the user's `tar` selection (the keyword being used
  // for quick-keys + ${target}). Two distinct concepts, same bar.
  const [combat, setCombat] = useState<CombatState | null>(null);
  const [userTarget, setUserTarget] = useState<string | null>(null);
  const [quickKeys, setQuickKeys] = useState<QuickKey[]>([]);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let unsubTarget: (() => void) | undefined;
    let cancelled = false;

    // Seed initial state — backend keeps quick-keys across sessions
    // but doesn't push an event until they change, so we'd otherwise
    // render an empty list until first edit.
    getTarget()
      .then((snap) => {
        if (cancelled) return;
        setUserTarget(snap.name);
        setQuickKeys(snap.quick_keys);
      })
      .catch(() => {});

    onGmcp((payload) => {
      if (payload.package !== 'Char.Combat') return;
      setCombat(extractCombat(payload.data));
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
        setCombat(null);
        setUserTarget(null);
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
  // Slot stays reserved at fixed height even when empty so the
  // bottom rail doesn't shift when combat starts or ends — xterm's
  // reflow lags layout changes by a frame and would otherwise cover
  // the terminal's bottom row.
  if (!combat && !userTarget && configuredKeys.length === 0) {
    return <div className="target-bar is-empty" aria-hidden="true" />;
  }
  const pct = combat?.hp;
  const fillColor = pct !== undefined ? colorForPercent(pct) : '#7aa89f';

  return (
    <div className="target-bar" aria-label="combat and target">
      {combat && (
        <>
          <span className="target-bar-swords" aria-hidden="true">
            ⚔⚔
          </span>
          <span className="target-bar-name">{combat.name}</span>
          {pct !== undefined ? (
            <div
              className="target-bar-meter"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={pct}
              aria-label={`target hp ${pct}%`}
            >
              <div
                className="target-bar-fill"
                style={{ width: `${pct}%`, background: fillColor }}
              />
              <span className="target-bar-pct">{pct}%</span>
            </div>
          ) : (
            <span className="target-bar-pct target-bar-pct-unknown">--</span>
          )}
          {combat.condition && (
            <span className="target-bar-condition">{combat.condition}</span>
          )}
        </>
      )}
      {(userTarget || configuredKeys.length > 0) && (
        <div className="target-bar-user">
          {combat && <span className="target-bar-sep" aria-hidden="true">|</span>}
          {userTarget && (
            <>
              <span className="target-bar-user-label">tar</span>
              <span className="target-bar-user-name">{userTarget}</span>
            </>
          )}
          {configuredKeys.length > 0 && (
            <>
              {userTarget && (
                <span className="target-bar-sep" aria-hidden="true">—</span>
              )}
              <ul className="target-bar-qkeys" aria-label="quick keys">
                {configuredKeys.map((qk, i) => (
                  <li key={qk.name} className="target-bar-qkey">
                    {i > 0 && (
                      <span className="target-bar-qkey-sep" aria-hidden="true">
                        ·
                      </span>
                    )}
                    <span className="target-bar-qkey-name">{qk.name}</span>
                    <span className="target-bar-qkey-verb">{qk.verb}</span>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
    </div>
  );
}
