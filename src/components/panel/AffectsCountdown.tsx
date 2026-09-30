import { useMemo, useRef, type CSSProperties } from 'react';
import {
  affectsPaneRows,
  gaugeFraction,
  hoursTone,
  type AffectFulls,
  type AffectInput,
  type AffectRow,
  type TrackedInput,
} from '../../lib/affectsView';
import type { AffectsMarker } from '../../lib/session';
import { useBoxSize, usePagedWindow, type Box } from './affectsHooks';
import { AffectMark, AffectsHeader, affectsEmpty, MoreButton } from './affectsParts';
import { COUNTDOWN_ROW_PX, countdownGrid, type CountdownGrid } from './countdownGrid';
import { affectHours, affectWords } from './paneText';

// Board Affects B, Countdown. One run by the hours left, missing first
// and permanent last, down the left column and on down the right, on
// 23 px rows. Each cell carries Timers first's mark, the name exactly
// as the game sends it, the hours at the right edge with the game's own
// `+` and `-`, and a 2 px meter under the text. The meter drains from
// full toward empty over the affect's own cast (gaugeFraction reads the
// full the backend keeps), in the tertiary tone, yellow at two hours and
// red at one or none. It stays full for a permanent affect, empty for a
// missing one, and is gone when the server sent no hours. What does not
// fit is the end of the countdown, counted in the last cell of a page,
// and a click on the count scrolls to it.

export interface CountdownViewProps {
  current: readonly AffectInput[] | null;
  tracked: readonly TrackedInput[];
  hidden: boolean;
  /** The body's size. The pane measures its own when left out. */
  box?: Box | undefined;
  marker?: AffectsMarker | undefined;
  /** Hours at full for each affect, from the affect full store. */
  full: AffectFulls;
}

/** The Countdown pane drawn from plain values, so each state renders
 *  in a test. */
export function CountdownView({
  current,
  tracked,
  hidden,
  box,
  marker = 'dot',
  full,
}: CountdownViewProps) {
  const rows = useMemo(() => affectsPaneRows(current, tracked, hidden), [current, tracked, hidden]);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const measured = useBoxSize(bodyRef);
  const size = box ?? measured;
  const grid = useMemo(() => countdownGrid(rows, size), [rows, size]);
  return (
    <>
      <AffectsHeader rows={rows} />
      <div
        ref={bodyRef}
        className="pane-body"
        data-affects-marker={marker === 'dot' ? undefined : marker}
      >
        {affectsEmpty(current, hidden, rows) ?? <CountdownPages grid={grid} full={full} />}
      </div>
    </>
  );
}

/** Every page of the countdown in a window one page tall. */
function CountdownPages({ grid, full }: { grid: CountdownGrid; full: AffectFulls }) {
  const pageHeight = grid.pageRows * COUNTDOWN_ROW_PX;
  const paged = usePagedWindow(pageHeight);
  return (
    <div
      ref={paged.ref}
      className={`pane-countdown-window${grid.pages > 1 ? ' is-paged' : ''}`}
      style={{ height: pageHeight }}
      onPointerLeave={paged.onPointerLeave}
      onBlur={paged.onBlur}
    >
      <ul
        className="pane-countdown"
        aria-label="Affects by hours left"
        style={{
          gridTemplateColumns: `repeat(${grid.columns}, minmax(0, 1fr))`,
          gridTemplateRows: `repeat(${grid.pages * grid.pageRows}, ${COUNTDOWN_ROW_PX}px)`,
        }}
      >
        {grid.cells.map((cell) => {
          const place: CSSProperties = { gridRow: cell.gridRow, gridColumn: cell.gridColumn };
          if (cell.kind === 'affect') {
            return (
              <CountdownCell
                key={cell.row.key}
                row={cell.row}
                full={full}
                pageStart={cell.pageStart}
                style={place}
              />
            );
          }
          return (
            <li key={`more-${cell.page}`} className="pane-countdown-more-cell" style={place}>
              <MoreButton count={cell.count} onClick={() => paged.toPage(cell.page + 1)} />
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function CountdownCell({
  row,
  full,
  pageStart,
  style,
}: {
  row: AffectRow;
  full: AffectFulls;
  pageStart: boolean;
  style: CSSProperties;
}) {
  const tone = hoursTone(row.ticks);
  const gauge = gaugeFraction(row, full);
  // The hours and the mark show only as glyphs and color, so a screen
  // reader hears them as words after the name.
  const words = affectWords(row.state, row.ticks);
  const cls = `pane-countdown-cell pane-affect-${row.state}${tone ? ` is-${tone}` : ''}${
    pageStart ? ' is-page-start' : ''
  }`;
  return (
    <li className={cls} style={style}>
      <AffectMark row={row} />
      <span className="pane-countdown-line">
        <span className="pane-countdown-name">
          {row.name}
          {words && <span className="pane-sr">{words}</span>}
        </span>
        <span className={`pane-countdown-hours${tone ? ` is-${tone}` : ''}`} aria-hidden="true">
          {affectHours(row.state, row.ticks)}
        </span>
      </span>
      {gauge !== null && (
        <span className="pane-countdown-meter" aria-hidden="true">
          <span className="pane-countdown-fill" style={{ width: `${(gauge * 100).toFixed(1)}%` }} />
        </span>
      )}
    </li>
  );
}
