import { useMemo } from 'react';
import {
  affectsPaneRows,
  isTrackedRow,
  type AffectInput,
  type AffectRow,
  type TrackedInput,
} from '../../lib/affectsView';
import { useAffects, useAffectsHidden } from '../../lib/stores/affectsStore';
import { useTrackedAffects } from '../../lib/stores/trackedAffectsStore';
import { PaneHeader, PaneMeta } from './PaneHeader';
import { affectStateWord, ticksLabel } from './paneText';

// The at a glance checklist (SPEC 9). Tracked affects you are missing
// come first with a hollow danger ring, then the tracked ones you have
// by ticks left, then everything else under Not tracked with harmful
// affects on top. affectsView orders the rows.
//
// While the game hides your affects (Char.Affects with the hidden flag,
// under lamented tears) the pane says so in place of the rows, and no
// tracked affect reads missing.

export function AffectsPane() {
  const current = useAffects();
  const tracked = useTrackedAffects();
  const hidden = useAffectsHidden();
  return <AffectsPaneView current={current} tracked={tracked} hidden={hidden} />;
}

export interface AffectsPaneViewProps {
  /** Your affects, or null until the server sends the list. */
  current: readonly AffectInput[] | null;
  tracked: readonly TrackedInput[];
  /** The game hides your affects. */
  hidden: boolean;
}

/** The pane drawn from plain values, so each state renders in a test. */
export function AffectsPaneView({ current, tracked, hidden }: AffectsPaneViewProps) {
  const rows = useMemo(() => affectsPaneRows(current, tracked, hidden), [current, tracked, hidden]);
  const trackedRows = rows.filter(isTrackedRow);
  const otherRows = rows.filter((r) => !isTrackedRow(r));
  const missing = trackedRows.filter((r) => r.state === 'missing').length;

  let body: React.ReactNode;
  if (hidden) {
    body = <p className="pane-empty">The game hides your affects right now.</p>;
  } else if (current === null) {
    body = <p className="pane-empty">Affects appear when you log in.</p>;
  } else if (rows.length === 0) {
    body = <p className="pane-empty">Nothing affects you right now.</p>;
  } else {
    body = (
      <>
        {trackedRows.length > 0 && <AffectList rows={trackedRows} />}
        {trackedRows.length > 0 && otherRows.length > 0 && (
          <>
            <div className="pane-divider" aria-hidden="true" />
            <h3 className="pane-sublabel">Not tracked</h3>
          </>
        )}
        {otherRows.length > 0 && <AffectList rows={otherRows} />}
      </>
    );
  }

  return (
    <>
      <PaneHeader
        meta={missing > 0 ? <PaneMeta tone="danger">{missing} missing</PaneMeta> : null}
      />
      <div className="pane-body">{body}</div>
    </>
  );
}

function AffectList({ rows }: { rows: AffectRow[] }) {
  return (
    <ul className="pane-rows">
      {rows.map((row) => {
        // Expiring and harmful show only as the marker color, so a
        // screen reader hears them as words after the name.
        const word = affectStateWord(row.state);
        return (
          <li key={row.key} className={`pane-row pane-row-marked pane-affect-${row.state}`}>
            <span className="pane-marker" aria-hidden="true" />
            <span className="pane-row-name">
              {row.name}
              {word && <span className="pane-sr">, {word}</span>}
            </span>
            <span className="pane-row-value">{ticksLabel(row.state, row.ticks)}</span>
          </li>
        );
      })}
    </ul>
  );
}
