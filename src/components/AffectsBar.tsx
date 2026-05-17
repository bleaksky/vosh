import { useEffect, useState } from 'react';
import {
  affectDescription,
  colorForDuration,
  formatDuration,
  groupAffects,
  type Affect,
  type GroupedAffect,
} from '../lib/affects';
import { onGmcp, onState } from '../lib/session';

// Thin row above the input that renders every active Char.Affects
// entry as a pill chip. Duration color tracks urgency (red imminent,
// orange < 5h, yellow < 10h, green > 10h, cyan permanent) so a
// sanctuary about to drop catches the eye without scrolling.
//
// Hides itself entirely when the affect list is empty so the layout
// does not reserve a slot during cleared/disconnected states.
export function AffectsBar() {
  const [groups, setGroups] = useState<GroupedAffect[]>([]);

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

  if (groups.length === 0) return null;

  return (
    <div className="affects-bar" aria-label="active affects">
      {groups.map((g) => {
        const title = [g.name, affectDescription(g as Affect)]
          .filter(Boolean)
          .join(' — ');
        return (
          <span key={g.name} className="affect-pill" title={title}>
            <span className="affect-pill-name">{g.name}</span>
            <span
              className="affect-pill-duration"
              style={{ color: colorForDuration(g.duration) }}
            >
              {formatDuration(g.duration)}
            </span>
          </span>
        );
      })}
    </div>
  );
}
