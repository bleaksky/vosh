import { useEffect, useState } from 'react';
import {
  affectDescription,
  colorForDuration,
  formatDuration,
  groupAffects,
  normalizeAffectName,
  type Affect,
  type GroupedAffect,
} from '../lib/affects';
import { getUiConfig, onGmcp, onState, subscribeTrackedAffectsChanged } from '../lib/session';
import { InlineTick } from './VitalsBar';

// Thin row above the input that renders every active Char.Affects
// entry as a pill chip. Duration color tracks urgency (red imminent,
// orange < 5h, yellow < 10h, green > 10h, cyan permanent) so a
// sanctuary about to drop catches the eye without scrolling.
//
// Hides itself entirely when the affect list is empty so the layout
// does not reserve a slot during cleared/disconnected states.
export function AffectsBar({ embedTick = false }: { embedTick?: boolean } = {}) {
  const [groups, setGroups] = useState<GroupedAffect[]>([]);
  const [tracked, setTracked] = useState<string[]>([]);

  // Seed + subscribe to tracked-affects list from the settings window.
  useEffect(() => {
    let cancelled = false;
    let unsubTracked: (() => void) | undefined;
    getUiConfig()
      .then((cfg) => {
        if (!cancelled) setTracked(cfg.tracked_affects ?? []);
      })
      .catch(() => undefined);
    subscribeTrackedAffectsChanged((list) => {
      if (!cancelled) setTracked(list);
    }).then((fn) => {
      if (cancelled) fn();
      else unsubTracked = fn;
    });
    return () => {
      cancelled = true;
      unsubTracked?.();
    };
  }, []);

  useEffect(() => {
    let unsubGmcp: (() => void) | undefined;
    let unsubState: (() => void) | undefined;
    let cancelled = false;

    onGmcp((payload) => {
      if (payload.package === 'Char.Affects' && payload.data && typeof payload.data === 'object') {
        const data = payload.data as { affects?: Affect[] };
        const list = Array.isArray(data.affects) ? data.affects : [];
        const grouped = groupAffects(list);
        grouped.sort((a, b) => a.name.localeCompare(b.name));
        setGroups(grouped);
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unsubGmcp = fn;
    });

    onState((payload) => {
      if (payload.kind === 'disconnected') setGroups([]);
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

  // Hidden until the user picks at least one affect to track. Each
  // tracked entry renders as a pill — live one shows the duration, a
  // missing one renders dim as "absent" so the user can see at a
  // glance which of their tracked buffs are NOT up.
  if (tracked.length === 0) return null;

  const liveByKey = new Map<string, GroupedAffect>();
  for (const g of groups) liveByKey.set(normalizeAffectName(g.name), g);

  return (
    <div className="affects-bar" aria-label="tracked affects">
      {tracked.map((name) => {
        const live = liveByKey.get(normalizeAffectName(name));
        if (live) {
          const title = [live.name, affectDescription(live as Affect)].filter(Boolean).join(' — ');
          return (
            <span key={name} className="affect-pill" title={title}>
              <span className="affect-pill-name">{live.name}</span>
              <span
                className="affect-pill-duration"
                style={{ color: colorForDuration(live.duration) }}
              >
                {formatDuration(live.duration)}
              </span>
            </span>
          );
        }
        return (
          <span key={name} className="affect-pill affect-pill-absent" title={`${name} not active`}>
            <span className="affect-pill-name">{name}</span>
            <span className="affect-pill-duration">—</span>
          </span>
        );
      })}
      {embedTick && <InlineTick className="affects-bar-tick" />}
    </div>
  );
}
