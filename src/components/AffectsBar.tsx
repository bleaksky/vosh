import { useEffect, useState } from 'react';
import {
  affectDescription,
  colorForDuration,
  formatDuration,
  normalizeAffectName,
  type Affect,
} from '../lib/affects';
import { getUiConfig, onGmcp, onState } from '../lib/session';

export function AffectsBar() {
  const [affects, setAffects] = useState<Affect[]>([]);
  const [tracked, setTracked] = useState<string[]>([]);

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
    let cancelled = false;

    onGmcp((payload) => {
      if (
        payload.package === 'Char.Affects' &&
        payload.data &&
        typeof payload.data === 'object'
      ) {
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
      if (payload.kind === 'disconnected') setAffects([]);
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

  if (tracked.length === 0) return null;

  const liveByKey = new Map<string, Affect>();
  for (const a of affects) liveByKey.set(normalizeAffectName(a.name), a);

  return (
    <div className="affects-bar" aria-label="tracked affects">
      {tracked.map((wanted) => {
        const live = liveByKey.get(normalizeAffectName(wanted));
        if (!live) {
          return (
            <span
              key={wanted}
              className="affect-pill affect-pill-missing"
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
        const desc = affectDescription(live);
        return (
          <span
            key={wanted}
            className="affect-pill"
            style={{ borderColor: color, color }}
            title={desc ?? `${live.name} ${formatDuration(dur)}`}
          >
            <span className="affect-pill-name">{live.name}</span>
            <span className="affect-pill-dur">{formatDuration(dur)}</span>
          </span>
        );
      })}
    </div>
  );
}
