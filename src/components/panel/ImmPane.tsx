import { useEffect, useState } from 'react';
import { getImmState, subscribeImmState, type ImmState } from '../../lib/immStore';
import { immRows, immSummary } from './immRows';
import { PaneHeader, PaneMeta } from './PaneHeader';

// The staff duty board from Imm.Queues, in dense rows: a marker in the
// deadline tier's color, the queue, a short aside, and the count. The
// server sends the package to immortals only.

export function ImmPane() {
  const [state, setState] = useState<ImmState>(() => getImmState());
  useEffect(() => subscribeImmState(setState), []);

  const rows = state.received ? immRows(state.queues) : [];
  const summary = state.received ? immSummary(state.queues) : null;

  return (
    <>
      <PaneHeader
        meta={
          summary ? (
            <PaneMeta tone={summary.tier === 'overdue' ? 'danger' : 'warn'}>
              {summary.text}
            </PaneMeta>
          ) : null
        }
      />
      <div className="pane-body">
        {!state.received ? (
          <p className="pane-empty">Staff queues appear when you log in as an immortal.</p>
        ) : rows.length === 0 ? (
          <p className="pane-empty">No staff queue needs you right now.</p>
        ) : (
          <ul className="pane-rows">
            {rows.map((row) => (
              <li
                key={row.key}
                className={`pane-row pane-row-marked pane-imm-${row.tier}`}
                title={row.title}
              >
                <span className="pane-marker" aria-hidden="true" />
                <span className="pane-row-name">
                  {row.label}
                  {row.note && <span className="pane-row-note">{row.note}</span>}
                </span>
                <span className="pane-row-value">{row.count}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </>
  );
}
