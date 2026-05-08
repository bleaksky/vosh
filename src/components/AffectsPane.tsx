import { useEffect, useState } from 'react';
import {
  affectDescription,
  colorForDuration,
  formatDuration,
  type Affect,
} from '../lib/affects';
import { onGmcp, onState } from '../lib/session';

export function AffectsPane() {
  const [affects, setAffects] = useState<Affect[]>([]);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let cancelled = false;

    onGmcp((payload) => {
      if (payload.package !== 'Char.Affects') return;
      const data = payload.data as { affects?: Affect[] } | undefined;
      const list = Array.isArray(data?.affects) ? data!.affects : [];
      // Dedupe by name preserving server order, then sort alphabetically
      // for a stable list (matches the user's tintin affects panel).
      const seen = new Set<string>();
      const deduped: Affect[] = [];
      for (const a of list) {
        if (!a?.name || seen.has(a.name)) continue;
        seen.add(a.name);
        deduped.push(a);
      }
      deduped.sort((x, y) => x.name.localeCompare(y.name));
      setAffects(deduped);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setAffects([]);
        setExpanded({});
      }
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

  const toggle = (name: string) => {
    setExpanded((prev) => ({ ...prev, [name]: !prev[name] }));
  };

  return (
    <section className="affects-pane" aria-label="affects">
      <header className="pane-header">affects</header>
      <div className="affects-body">
        {affects.length === 0 ? (
          <div className="affects-empty">no active affects</div>
        ) : (
          <ul className="affects-list">
            {affects.map((affect) => {
              const desc = affectDescription(affect);
              const isOpen = expanded[affect.name] ?? false;
              const color = colorForDuration(
                typeof affect.duration === 'number'
                  ? affect.duration
                  : affect.duration !== undefined
                    ? Number(affect.duration)
                    : undefined,
              );
              return (
                <li key={affect.name} className="affect-row">
                  <button
                    type="button"
                    className="affect-row-head"
                    onClick={() => toggle(affect.name)}
                    aria-expanded={isOpen}
                  >
                    <span className="affect-row-toggle" aria-hidden="true">
                      {desc ? (isOpen ? '▾' : '▸') : '·'}
                    </span>
                    <span className="affect-row-name">{affect.name}</span>
                    {affect.stacks !== undefined && Number(affect.stacks) > 1 && (
                      <span className="affect-row-stacks">×{affect.stacks}</span>
                    )}
                    <span className="affect-row-dur" style={{ color }}>
                      {formatDuration(
                        typeof affect.duration === 'number'
                          ? affect.duration
                          : affect.duration !== undefined
                            ? Number(affect.duration)
                            : undefined,
                      )}
                    </span>
                  </button>
                  {isOpen && desc && <div className="affect-row-desc">{desc}</div>}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </section>
  );
}
