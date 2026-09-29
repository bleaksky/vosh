import { useMemo } from 'react';
import { affectsView, isTrackedRow, type AffectRow } from '../../lib/affectsView';
import { useAffects } from '../../lib/stores/affectsStore';
import { useTrackedAffects } from '../../lib/stores/trackedAffectsStore';
import { PaneHeader, PaneMeta } from './PaneHeader';
import { ticksLabel } from './paneText';

// The at a glance checklist (SPEC 9). Tracked affects you are missing
// come first with a hollow danger ring, then the tracked ones you have
// by ticks left, then everything else under Not tracked with harmful
// affects on top. affectsView orders the rows.

export function AffectsPane() {
  const current = useAffects();
  const tracked = useTrackedAffects();
  const rows = useMemo(
    () => (current === null ? [] : affectsView(current, tracked)),
    [current, tracked],
  );
  const trackedRows = rows.filter(isTrackedRow);
  const otherRows = rows.filter((r) => !isTrackedRow(r));
  const missing = trackedRows.filter((r) => r.state === 'missing').length;

  let body: React.ReactNode;
  if (current === null) {
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
      {rows.map((row) => (
        <li key={row.key} className={`pane-row pane-row-marked pane-affect-${row.state}`}>
          <span className="pane-marker" aria-hidden="true" />
          <span className="pane-row-name">{row.name}</span>
          <span className="pane-row-value">{ticksLabel(row.state, row.ticks)}</span>
        </li>
      ))}
    </ul>
  );
}
