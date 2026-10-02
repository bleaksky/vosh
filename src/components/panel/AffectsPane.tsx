import { useMemo, useRef, type CSSProperties } from 'react';
import { affectThresholdsOf } from '../../lib/affectsDisplay';
import {
  affectsPaneRows,
  DEFAULT_AFFECT_THRESHOLDS,
  type AffectInput,
  type AffectRow,
  type AffectThresholds,
  type TrackedInput,
} from '../../lib/affectsView';
import { PANE_ROW_PX } from '../../lib/paneLayout';
import type { AffectsMarker } from '../../lib/session';
import { useAffectFull } from '../../lib/stores/affectFullStore';
import { useAffectsDisplay } from '../../lib/stores/affectsDisplayStore';
import { useAffects, useAffectsHidden } from '../../lib/stores/affectsStore';
import { useTrackedAffects } from '../../lib/stores/trackedAffectsStore';
import { ChipsView } from './AffectsChips';
import { CountdownView } from './AffectsCountdown';
import { affectsGrid, type AffectsCell } from './affectsGrid';
import { useBoxSize, usePagedWindow, type Box } from './affectsHooks';
import { AffectMark, AffectsEmpty, AffectsHeader, MoreButton } from './affectsParts';
import { affectHours, affectsEmptyText, affectWords } from './paneText';

// The at a glance checklist, board Affects A, timers first. Two columns
// of 22 px rows, each the hours left in a right aligned column and then
// the name exactly as the game sends it, both in the terminal face. The
// column is three digits wide, wider while a longer count shows. The
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
//
// The mark beside each tracked affect is the one you pick in Settings,
// Layout, Affects or the pane menu: the dot, a square, plus and minus,
// or none. The body names any but the dot in data-affects-marker, and
// panel.css draws the shape in the color of the state.
//
// Tint what to recast, there too, washes a missing row red and one
// about to drop yellow or red. The body carries data-affects-tint while
// it is on.
//
// The hours at which an affect runs out and is almost gone come from
// there as well, two and one unless you change them, and every style
// colors the hours, the marks, the header counts and what to recast by
// them.
//
// Timers first is one of four styles you pick there. Countdown
// (AffectsCountdown.tsx) lists every affect by the hours it has left,
// and Grouped chips (AffectsChips.tsx) puts what to recast first.
// Draining chips draws the same chips, and a chip running out colors
// only the share that matches the hours it has left.

export function AffectsPane() {
  const current = useAffects();
  const tracked = useTrackedAffects();
  const hidden = useAffectsHidden();
  const display = useAffectsDisplay();
  const full = useAffectFull();
  // The store keeps one display while nothing in it moves, so the views
  // keep their rows.
  const thresholds = useMemo(() => affectThresholdsOf(display), [display]);
  if (display.style === 'chips' || display.style === 'chips_drain') {
    return (
      <ChipsView
        current={current}
        tracked={tracked}
        hidden={hidden}
        full={full}
        thresholds={thresholds}
        fill={display.style === 'chips_drain' ? 'drain' : 'tint'}
      />
    );
  }
  if (display.style === 'countdown') {
    return (
      <CountdownView
        current={current}
        tracked={tracked}
        hidden={hidden}
        marker={display.marker}
        tint={display.tint}
        full={full}
        thresholds={thresholds}
      />
    );
  }
  return (
    <AffectsPaneView
      current={current}
      tracked={tracked}
      hidden={hidden}
      marker={display.marker}
      tint={display.tint}
      thresholds={thresholds}
    />
  );
}

export interface AffectsPaneViewProps {
  /** Your affects, or null until the server sends the list. */
  current: readonly AffectInput[] | null;
  tracked: readonly TrackedInput[];
  /** The game hides your affects. */
  hidden: boolean;
  /** The body's size. The pane measures its own when left out, so a
   *  test passes one to draw what fits. */
  box?: Box | undefined;
  /** The mark beside each tracked affect. The dot when left out. */
  marker?: AffectsMarker | undefined;
  /** Wash the rows to recast, missing and running out. */
  tint?: boolean | undefined;
  /** When an affect runs out and is almost gone. Two and one hours
   *  when left out. */
  thresholds?: AffectThresholds | undefined;
}

/** The pane drawn from plain values, so each state renders in a test. */
export function AffectsPaneView({
  current,
  tracked,
  hidden,
  box,
  marker = 'dot',
  tint = false,
  thresholds = DEFAULT_AFFECT_THRESHOLDS,
}: AffectsPaneViewProps) {
  const rows = useMemo(
    () => affectsPaneRows(current, tracked, hidden, thresholds),
    [current, tracked, hidden, thresholds],
  );
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const measured = useBoxSize(bodyRef);
  const size = box ?? measured;
  const grid = useMemo(() => affectsGrid(rows, size), [rows, size]);
  // Every cell's hours column fits the longest count, three cells or
  // more, so the names stay in line and a 1200 hour psalm never runs
  // into its name.
  const hoursCh = useMemo(
    () => Math.max(3, ...rows.map((r) => affectHours(r.state, r.ticks).length)),
    [rows],
  );
  const hoursColumn: CSSProperties = { ['--affect-hours-ch' as string]: hoursCh };

  const empty = affectsEmptyText(current, hidden, rows);
  let body: React.ReactNode = empty === null ? null : <AffectsEmpty text={empty} />;
  if (empty === null) {
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
      <AffectsHeader rows={rows} />
      <div
        ref={bodyRef}
        className="pane-body"
        style={hoursColumn}
        data-affects-marker={marker === 'dot' ? undefined : marker}
        data-affects-tint={tint ? '' : undefined}
      >
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
  const pageHeight = pageRows * PANE_ROW_PX;
  // Back to the first page once you point and tab away, so the pane
  // shows the affects that matter most again.
  const paged = usePagedWindow(pageHeight);

  return (
    <div
      ref={paged.ref}
      className={`pane-affects-rest${pages > 1 ? ' is-paged' : ''}`}
      style={{ height: pageHeight }}
      onPointerLeave={paged.onPointerLeave}
      onBlur={paged.onBlur}
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
              <span className="pane-affect-hours" aria-hidden="true" />
              <MoreButton count={cell.count} onClick={() => paged.toPage(cell.page + 1)} />
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
  const tone = row.tone;
  // The hours and the mark show only as glyphs and color, so a screen
  // reader hears them as words after the name.
  const words = affectWords(row.state, row.ticks);
  return (
    <li
      className={`pane-affect pane-affect-${row.state}${pageStart ? ' is-page-start' : ''}`}
      style={style}
    >
      <AffectMark row={row} />
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
