import { useEffect, useState } from 'react';
import { onGmcpPackage, onState } from './session';

// Merged snapshot of every field the server pushes via Char.Vitals
// (hp, mana, move, position, alignment, ...) and Char.Worth (gold,
// silver, xp, tnl, practices, trains, ...). Used by the vitals
// template renderer so a user-authored template can interpolate ANY
// field the server ships, not just the curated `%hp` / `%mn` / etc.
// tokens — the template gets full pass-through access.
//
// Pushes are merged so a server that updates only Char.Vitals at one
// tick and only Char.Worth at another keeps both halves of the
// snapshot live. Disconnect clears the record so stale values do
// not show across sessions.
export type CharStats = Record<string, string | number | boolean | null | undefined>;

export function useCharStats(): CharStats {
  const [stats, setStats] = useState<CharStats>({});

  useEffect(() => {
    let cancelled = false;
    const unsubs: Array<() => void> = [];
    let unsubState: (() => void) | undefined;
    const merge = (data: unknown) => {
      if (!data || typeof data !== 'object') return;
      setStats((prev) => ({ ...prev, ...(data as CharStats) }));
    };
    const track = (p: Promise<() => void>) =>
      p.then((fn) => {
        if (cancelled) fn();
        else unsubs.push(fn);
      });

    track(onGmcpPackage<CharStats>('Char.Vitals', merge));
    track(onGmcpPackage<CharStats>('Char.Worth', merge));

    onState((p) => {
      if (p.kind === 'disconnected') setStats({});
    }).then((fn) => {
      if (cancelled) fn();
      else unsubState = fn;
    });

    return () => {
      cancelled = true;
      for (const fn of unsubs) fn();
      unsubState?.();
    };
  }, []);

  return stats;
}
