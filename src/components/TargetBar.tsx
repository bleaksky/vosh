import { useEffect, useState } from 'react';
import { onGmcp, onState } from '../lib/session';

interface TargetState {
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
function extractTarget(data: unknown): TargetState | null {
  if (!data || typeof data !== 'object') return null;
  const obj = data as Record<string, unknown>;
  const name = typeof obj.target === 'string' ? obj.target.trim() : '';
  if (!name) return null;
  const out: TargetState = { name };
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
  const [target, setTarget] = useState<TargetState | null>(null);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let cancelled = false;

    onGmcp((payload) => {
      if (payload.package !== 'Char.Combat') return;
      setTarget(extractTarget(payload.data));
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') setTarget(null);
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

  if (!target) return null;
  const pct = target.hp;
  const fillColor = pct !== undefined ? colorForPercent(pct) : '#7aa89f';

  return (
    <div className="target-bar" aria-label="combat target">
      <span className="target-bar-swords" aria-hidden="true">
        ⚔⚔
      </span>
      <span className="target-bar-name">{target.name}</span>
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
      {target.condition && (
        <span className="target-bar-condition">{target.condition}</span>
      )}
    </div>
  );
}
