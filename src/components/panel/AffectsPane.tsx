import { useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import {
  affectMark,
  affectsPaneRows,
  affectsSummary,
  hoursTone,
  type AffectInput,
  type AffectRow,
  type TrackedInput,
} from '../../lib/affectsView';
import { PANE_ROW_PX } from '../../lib/paneLayout';
import { useAffects, useAffectsHidden } from '../../lib/stores/affectsStore';
import { useTrackedAffects } from '../../lib/stores/trackedAffectsStore';
import { affectsGrid, type AffectsCell } from './affectsGrid';
import { PaneHeader, PaneMeta } from './PaneHeader';
import { affectHours, affectWords } from './paneText';

// The at a glance checklist, board Affects A, timers first. Two columns
// of 22 px rows, each the hours left in a right aligned column and then
// the name exactly as the game sends it, both in the terminal face. The
// game's own marks stand in for the hours, `+` permanent and `-`
// missing. Your tracked affects keep the slots you set in Characters,
// each with a dot that agrees with its hours. The rest sit under a
// hairline, harmful ones first. affectsView orders the rows and
// affectsGrid places them.
//
// The pane shows whole rows only. When the rest do not fit, the last
// cell counts the ones that do not, and a click on it scrolls them into
// view one page at a time. Pointing away scrolls back, so the harmful
// affects and the ones about to drop are in view again at a glance.
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

type Box = { width: number; height: number };

export interface AffectsPaneViewProps {
  /** Your affects, or null until the server sends the list. */
  current: readonly AffectInput[] | null;
  tracked: readonly TrackedInput[];
  /** The game hides your affects. */
  hidden: boolean;
  /** The body's size. The pane measures its own when left out, so a
   *  test passes one to draw what fits. */
  box?: Box | undefined;
}

/** The pane drawn from plain values, so each state renders in a test. */
export function AffectsPaneView({ current, tracked, hidden, box }: AffectsPaneViewProps) {
  const rows = useMemo(() => affectsPaneRows(current, tracked, hidden), [current, tracked, hidden]);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const measured = useBoxSize(bodyRef);
  const size = box ?? measured;
  const grid = useMemo(() => affectsGrid(rows, size), [rows, size]);
  const { missing, runningOut } = affectsSummary(rows);

  let body: React.ReactNode;
  if (hidden) {
    body = <p className="pane-empty">The game hides your affects right now.</p>;
  } else if (current === null) {
    body = <p className="pane-empty">Affects appear when you log in.</p>;
  } else if (rows.length === 0) {
    body = <p className="pane-empty">Nothing affects you right now.</p>;
  } else {
    const columns: CSSProperties = {
      gridTemplateColumns: `repeat(${grid.columns}, minmax(0, 1fr))`,
    };
    body = (
      <>
        {grid.tracked.length > 0 && (
          <ul className="pane-affects-grid" aria-label="Tracked, in your order" style={columns}>
            {grid.tracked.map((row) => (
              <AffectCell key={row.key} row={row} />
            ))}
          </ul>
        )}
        {grid.rule && <div className="pane-affects-rule" aria-hidden="true" />}
        {grid.rest.length > 0 && (
          <RestPages
            cells={grid.rest}
            pageRows={grid.pageRows}
            pages={grid.pages}
            columns={columns}
          />
        )}
      </>
    );
  }

  return (
    <>
      <PaneHeader
        meta={
          missing > 0 || runningOut > 0 ? (
            <>
              {missing > 0 && <PaneMeta tone="danger">{missing} missing</PaneMeta>}
              {runningOut > 0 && <PaneMeta tone="warn">{runningOut} running out</PaneMeta>}
            </>
          ) : null
        }
      />
      <div ref={bodyRef} className="pane-body">
        {body}
      </div>
    </>
  );
}

/** The affects you do not track, in a window `pageRows` rows tall that
 *  scrolls a page at a time. */
function RestPages({
  cells,
  pageRows,
  pages,
  columns,
}: {
  cells: AffectsCell[];
  pageRows: number;
  pages: number;
  columns: CSSProperties;
}) {
  const windowRef = useRef<HTMLDivElement | null>(null);
  const pageHeight = pageRows * PANE_ROW_PX;
  const behavior = (): ScrollBehavior =>
    window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth';
  // Back to the first page once you point and tab away, so the pane
  // shows the affects that matter most again.
  const home = () => {
    const el = windowRef.current;
    if (!el || el.scrollTop === 0) return;
    if (el.matches(':hover') || el.contains(document.activeElement)) return;
    el.scrollTo({ top: 0, behavior: behavior() });
  };

  return (
    <div
      ref={windowRef}
      className={`pane-affects-rest${pages > 1 ? ' is-paged' : ''}`}
      style={{ height: pageHeight }}
      onPointerLeave={home}
      onBlur={() => requestAnimationFrame(home)}
    >
      <ul
        className="pane-affects-grid"
        aria-label="Not tracked, by hours left"
        style={{ ...columns, gridTemplateRows: `repeat(${pages * pageRows}, ${PANE_ROW_PX}px)` }}
      >
        {cells.map((cell) => {
          const place: CSSProperties = { gridRow: cell.gridRow, gridColumn: cell.gridColumn };
          if (cell.kind === 'affect') {
            return (
              <AffectCell
                key={cell.row.key}
                row={cell.row}
                pageStart={cell.pageStart}
                style={place}
              />
            );
          }
          return (
            <li key={`more-${cell.page}`} className="pane-affects-more-cell" style={place}>
              <button
                type="button"
                className="pane-affects-more"
                aria-label={`${cell.count} more affects, scroll to them`}
                onClick={() =>
                  windowRef.current?.scrollTo({
                    top: (cell.page + 1) * pageHeight,
                    behavior: behavior(),
                  })
                }
              >
                {cell.count} more
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function AffectCell({
  row,
  pageStart = false,
  style,
}: {
  row: AffectRow;
  pageStart?: boolean;
  style?: CSSProperties;
}) {
  const mark = affectMark(row);
  const tone = hoursTone(row.ticks);
  // The hours and the mark show only as glyphs and color, so a screen
  // reader hears them as words after the name.
  const words = affectWords(row.state, row.ticks);
  return (
    <li
      className={`pane-affect pane-affect-${row.state}${pageStart ? ' is-page-start' : ''}`}
      style={style}
    >
      {mark && <span className={`pane-affect-mark is-${mark}`} aria-hidden="true" />}
      <span className={`pane-affect-hours${tone ? ` is-${tone}` : ''}`} aria-hidden="true">
        {affectHours(row.state, row.ticks)}
      </span>
      <span className="pane-affect-name">
        {row.name}
        {words && <span className="pane-sr">{words}</span>}
      </span>
    </li>
  );
}

/** The element's inner size, kept current as it resizes. Null until
 *  the first measure. */
function useBoxSize(ref: React.RefObject<HTMLElement | null>): Box | null {
  const [box, setBox] = useState<Box | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const width = el.clientWidth;
      const height = el.clientHeight;
      setBox((prev) =>
        prev && prev.width === width && prev.height === height ? prev : { width, height },
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return box;
}
