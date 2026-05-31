import { useEffect, useState } from 'react';
import { onGmcpPackage, onState } from './session';

export interface CombatState {
  name: string;
  hp?: number;
  condition?: string;
}

// Subscribe to Char.Combat GMCP pushes. Returns the current combat
// target (name, hp%, condition) or null when no combat is in progress.
// Disconnect clears the state.
export function useCombat(): CombatState | null {
  const [combat, setCombat] = useState<CombatState | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;

    onGmcpPackage<unknown>('Char.Combat', (data) => {
      setCombat(extractCombat(data));
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onState((p) => {
      if (p.kind === 'disconnected') setCombat(null);
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

  return combat;
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
