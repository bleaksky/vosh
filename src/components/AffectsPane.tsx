import { useEffect, useRef, useState } from 'react';
import {
  colorForDuration,
  formatDuration,
  formatModifier,
  groupAffects,
  type Affect,
  type GroupedAffect,
} from '../lib/affects';
import { onGmcp, onState } from '../lib/session';
import { useDockTarget } from '../lib/docking';

export function AffectsPane() {
  const [groups, setGroups] = useState<GroupedAffect[]>([]);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const dockRef = useRef<HTMLElement>(null);
  useDockTarget(dockRef, 'affects-pane');

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let cancelled = false;

    onGmcp((payload) => {
      if (payload.package !== 'Char.Affects') return;
      const data = payload.data as { affects?: Affect[] } | undefined;
      const list = Array.isArray(data?.affects) ? data!.affects : [];
      const grouped = groupAffects(list);
      grouped.sort((a, b) => a.name.localeCompare(b.name));
      setGroups(grouped);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') {
        setGroups([]);
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
    <section ref={dockRef} className="affects-pane" aria-label="affects">
      <header className="pane-header">affects</header>
      <div className="affects-body">
        {groups.length === 0 ? (
          <div className="affects-empty">no active affects</div>
        ) : (
          <ul className="affects-list">
            {groups.map((group) => {
              const isOpen = expanded[group.name] ?? false;
              const hasDetails = group.modifiers.length > 0 || !!group.description;
              const color = colorForDuration(group.duration);
              return (
                <li key={group.name} className="affect-row">
                  <button
                    type="button"
                    className="affect-row-head"
                    onClick={() => toggle(group.name)}
                    aria-expanded={isOpen}
                    disabled={!hasDetails}
                  >
                    <span className="affect-row-toggle" aria-hidden="true">
                      {hasDetails ? (isOpen ? '▾' : '▸') : '·'}
                    </span>
                    <span className="affect-row-name">{group.name}</span>
                    <span className="affect-row-dur" style={{ color }}>
                      {formatDuration(group.duration)}
                    </span>
                  </button>
                  {isOpen && hasDetails && (
                    <div className="affect-row-detail">
                      {group.modifiers.length > 0 && (
                        <ul className="affect-mods">
                          {group.modifiers.map((mod, i) => (
                            <li key={`${mod.location}:${i}`} className="affect-mod">
                              <span className="affect-mod-loc">{mod.location}</span>
                              <span
                                className={`affect-mod-val${
                                  Number(mod.modifier) > 0 ? ' is-positive' : ''
                                }${Number(mod.modifier) < 0 ? ' is-negative' : ''}`}
                              >
                                {formatModifier(mod.modifier)}
                              </span>
                            </li>
                          ))}
                        </ul>
                      )}
                      {group.description && (
                        <div className="affect-desc">{group.description}</div>
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </section>
  );
}
