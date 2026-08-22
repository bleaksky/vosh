import { useEffect, useState } from 'react';
import {
  affectDescription,
  colorForDuration,
  durationFraction,
  formatDuration,
  groupAffects,
  normalizeAffectName,
  type Affect,
  type GroupedAffect,
} from '../lib/affects';
import {
  getUiConfig,
  onGmcpPackage,
  onState,
  subscribeTrackedAffectsChanged,
  type TrackedAffect,
} from '../lib/session';

// Tracked-affects display with two shapes, both fed by Char.Affects:
//
// - 'strip' (default): thin row above the input that renders each
//   tracked affect as a pill chip. Duration color tracks urgency
//   (red imminent, orange < 5h, yellow < 10h, green > 10h, cyan
//   permanent) so a sanctuary about to drop catches the eye without
//   scrolling.
// - 'rows': Ember sidebar pane for side-zone placement. Pane head
//   (caps "affects" + mono active count) over one row per tracked
//   affect: mono name, 30px duration mini-bar filled by fraction of
//   a day remaining, mono countdown. Fill and countdown share the
//   same urgency ladder color.
//
// Hides itself entirely until the user tracks at least one affect so
// the layout does not reserve a slot during cleared/disconnected
// states.
export function AffectsBar({ variant = 'strip' }: { variant?: 'strip' | 'rows' }) {
  const [groups, setGroups] = useState<GroupedAffect[]>([]);
  const [tracked, setTracked] = useState<TrackedAffect[]>([]);

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

    onGmcpPackage<{ affects?: Affect[] }>('Char.Affects', (data) => {
      if (!data || typeof data !== 'object') return;
      const list = Array.isArray(data.affects) ? data.affects : [];
      const grouped = groupAffects(list);
      grouped.sort((a, b) => a.name.localeCompare(b.name));
      setGroups(grouped);
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

  // Hidden until the user picks at least one affect to track. A live
  // tracked entry shows its duration; a missing one renders dim as
  // "absent" so the user can see at a glance which of their tracked
  // buffs are NOT up.
  if (tracked.length === 0) return null;

  const liveByKey = new Map<string, GroupedAffect>();
  for (const g of groups) liveByKey.set(normalizeAffectName(g.name), g);

  if (variant === 'rows') {
    const liveCount = tracked.filter((t) => liveByKey.has(normalizeAffectName(t.name))).length;
    return (
      <div className="affects-rows" aria-label="tracked affects">
        <div className="affects-rows-head">
          <span className="caps">affects</span>
          <span className="affects-rows-count">{liveCount}</span>
        </div>
        {tracked.map((entry, i) => {
          const live = liveByKey.get(normalizeAffectName(entry.name));
          if (!live) {
            return (
              <div
                key={`${entry.name}-${i}`}
                className="affect-row affect-row-absent"
                title={`${entry.name} not active`}
              >
                <span className="affect-row-name">{displayName(entry)}</span>
                <span className="affect-row-bar" />
                <span className="affect-row-time">—</span>
              </div>
            );
          }
          const color = colorForDuration(live.duration);
          return (
            <div key={`${entry.name}-${i}`} className="affect-row" title={liveTitle(entry, live)}>
              <span className="affect-row-name">{displayName(entry)}</span>
              <span className="affect-row-bar">
                <span
                  className="affect-row-fill"
                  style={{ width: `${durationFraction(live.duration) * 100}%`, background: color }}
                />
              </span>
              <span className="affect-row-time" style={{ color }}>
                {formatDuration(live.duration)}
              </span>
            </div>
          );
        })}
      </div>
    );
  }

  return (
    <div className="affects-bar" aria-label="tracked affects">
      {tracked.map((entry, i) => {
        const live = liveByKey.get(normalizeAffectName(entry.name));
        if (live) {
          return (
            <span key={`${entry.name}-${i}`} className="affect-pill" title={liveTitle(entry, live)}>
              <span className="affect-pill-name">{displayName(entry)}</span>
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
          <span
            key={`${entry.name}-${i}`}
            className="affect-pill affect-pill-absent"
            title={`${entry.name} not active`}
          >
            <span className="affect-pill-name">{displayName(entry)}</span>
            <span className="affect-pill-duration">—</span>
          </span>
        );
      })}
    </div>
  );
}

// Display label falls back to the in-world server name when the user
// hasn't set one. The tooltip always shows the server-side identity
// so a custom label cannot hide what the affect actually is.
function displayName(entry: TrackedAffect): string {
  return entry.label && entry.label.length > 0 ? entry.label : entry.name;
}

function liveTitle(entry: TrackedAffect, live: GroupedAffect): string {
  return [
    live.name === entry.name ? live.name : `${live.name} (tracked as ${entry.name})`,
    affectDescription(live as Affect),
  ]
    .filter(Boolean)
    .join(' — ');
}
