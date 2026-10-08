import { useMemo, useRef, type CSSProperties } from 'react';
import {
  affectsPaneRows,
  DEFAULT_AFFECT_THRESHOLDS,
  gaugeFraction,
  type AffectInput,
  type AffectRow,
  type AffectThresholds,
  type TrackedInput,
} from './affectsView';
import type { AffectFulls } from '../../ipc/affects';
import { useBoxSize, usePagedWindow, type Box } from './affectsHooks';
import { AffectsEmpty, AffectsHeader, MoreButton } from './affectsParts';
import { useChipMeasure } from './chipMeasure';
import {
  chipGroups,
  chipKind,
  chipLabelMode,
  chipPages,
  chipTone,
  chipDots,
  chipDotsPath,
  chipWidth,
  CHIP_DOT_PX,
  type ChipGroup,
  type ChipMeasure,
  type ChipPage,
  type LabelMode,
} from './chipsGrid';
import { affectHours, affectsEmptyText, affectWords } from '../paneText';
import { usePaneText } from '../paneTextSize';

// Board Affects C, Grouped chips. What to recast first: the tracked
// affects you miss and the ones running out, then the rest you track,
// then everything else, each a chip with the name exactly as the game
// sends it and the hours after it. chipsGrid packs the lines and the
// pages, and the pane draws exactly what it packs. The chips and the
// lines follow your panel size.
//
// Each chip carries its own state, so C draws no marker. A missing
// affect is a chip with no ground, ringed in soft red dots (ChipDots).
// A tracked chip is filled, and the fill is a gauge: it drains from
// the left toward empty as the hours run down, over the affect's own
// cast (gaugeFraction, from the fulls the backend keeps), with a
// hairline to show the chip's full width. One running out takes the
// warn yellow, and red once it is almost gone, over the whole chip,
// at the hours you set, two and one unless you change them. Its gauge
// shows stronger over that while it drains. Other chips keep the
// hairline ring, and a harmful one its danger ring.
//
// Draining chips (fill drain) is the same pane, and only a chip running
// out draws differently: no tint over the whole chip, a hairline for
// its full width, and the yellow or red only over the share that
// matches the hours it has left. The body carries data-chip-fill, and
// affects.css draws the rest. On a light theme it eases the red fill and
// draws yellow hours in the warn text tone, so the hours read over the
// fill. Dark themes stay as drawn.

export interface ChipsViewProps {
  current: readonly AffectInput[] | null;
  tracked: readonly TrackedInput[];
  hidden: boolean;
  /** The body's size. The pane measures its own when left out. */
  box?: Box | undefined;
  /** Hours at full for each affect, from the affect full store. */
  full: AffectFulls;
  /** Text widths. The live faces when left out. */
  measure?: ChipMeasure | undefined;
  /** When an affect runs out and is almost gone. Two and one hours
   *  when left out. */
  thresholds?: AffectThresholds | undefined;
  /** How a chip running out colors: `tint` over the whole chip, as
   *  Grouped chips draws it and the default, or `drain`, as Draining
   *  chips draws it, only over the hours it has left. */
  fill?: ChipFill | undefined;
}

export type ChipFill = 'tint' | 'drain';

const hoursOf = (row: AffectRow) => affectHours(row.state, row.ticks);

/** The Grouped chips pane drawn from plain values, so each state
 *  renders in a test. */
export function ChipsView({
  current,
  tracked,
  hidden,
  box,
  full,
  measure,
  thresholds = DEFAULT_AFFECT_THRESHOLDS,
  fill = 'tint',
}: ChipsViewProps) {
  const rows = useMemo(
    () => affectsPaneRows(current, tracked, hidden, thresholds),
    [current, tracked, hidden, thresholds],
  );
  const empty = affectsEmptyText(current, hidden, rows);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const measured = useBoxSize(bodyRef);
  const size = box ?? measured;
  const text = usePaneText();
  const live = useChipMeasure(text.size);
  const m = measure ?? live;
  const groups = useMemo(() => chipGroups(rows), [rows]);
  // Before the body is measured, every chip goes on one page.
  const width = size?.width ?? 494;
  const height = size?.height ?? Number.POSITIVE_INFINITY;
  const labels = chipLabelMode(rows, width, text.size);
  const pages = useMemo(
    () => chipPages(groups, width, hoursOf, m, labels, height, text.size),
    [groups, width, m, labels, height, text.size],
  );
  return (
    <>
      <AffectsHeader rows={rows} />
      <div
        ref={bodyRef}
        className="pane-body"
        data-chip-fill={fill === 'drain' ? 'drain' : undefined}
      >
        {empty !== null ? (
          <AffectsEmpty text={empty} />
        ) : (
          <ChipPages
            groups={groups}
            pages={pages}
            labels={labels}
            height={size ? size.height : null}
            full={full}
            measure={m}
            chipH={text.chip}
          />
        )}
      </div>
    </>
  );
}

/** Every page, each as tall as the body, stacked in a window that
 *  scrolls a page at a time. */
function ChipPages({
  groups,
  pages,
  labels,
  height,
  full,
  measure,
  chipH,
}: {
  groups: readonly ChipGroup[];
  pages: ChipPage[];
  labels: LabelMode;
  height: number | null;
  full: AffectFulls;
  measure: ChipMeasure;
  /** A chip line's height. */
  chipH: number;
}) {
  const pageHeight = height ?? 0;
  const paged = usePagedWindow(pageHeight);
  const labelOf = (id: string) => groups.find((g) => g.id === id)?.label ?? '';
  const tall: CSSProperties | undefined = height === null ? undefined : { height };
  return (
    <div
      ref={paged.ref}
      className={`pane-chips${pages.length > 1 ? ' is-paged' : ''}`}
      style={tall}
      onPointerLeave={paged.onPointerLeave}
      onBlur={paged.onBlur}
    >
      {pages.map((page, p) => (
        <div
          key={p}
          className="pane-chips-page"
          style={height === null ? { height: pageBottom(page, chipH) } : tall}
        >
          {page.lines.map((line, l) => {
            const last = l === page.lines.length - 1;
            if (line.rows.length === 0) {
              // A run in name alone. The lines under it name the group.
              return (
                <div
                  key={`${line.group}-${line.top}`}
                  className="pane-chips-line"
                  aria-hidden="true"
                  style={{ top: line.top }}
                >
                  <span className="pane-chips-label pane-chips-runin">{labelOf(line.group)}</span>
                </div>
              );
            }
            return (
              <ul
                key={`${line.group}-${line.top}`}
                className="pane-chips-line"
                aria-label={labelOf(line.group)}
                style={{ top: line.top }}
              >
                {labels === 'gutter' && (
                  <li className="pane-chips-label" aria-hidden="true">
                    {line.labelled ? labelOf(line.group) : ''}
                  </li>
                )}
                {labels === 'runin' && line.labelled && (
                  <li className="pane-chips-label pane-chips-runin" aria-hidden="true">
                    {labelOf(line.group)}
                  </li>
                )}
                {line.rows.map((row) => (
                  <Chip key={row.key} row={row} full={full} measure={measure} chipH={chipH} />
                ))}
                {last && page.more > 0 && (
                  <li className="pane-chips-more">
                    <MoreButton count={page.more} onClick={() => paged.toPage(p + 1)} />
                  </li>
                )}
              </ul>
            );
          })}
        </div>
      ))}
    </div>
  );
}

/** A page's height before the body is measured: down to its last
 *  line, `chipH` tall. */
function pageBottom(page: ChipPage, chipH: number): number {
  const last = page.lines[page.lines.length - 1];
  return last ? last.top + chipH : 0;
}

function Chip({
  row,
  full,
  measure,
  chipH,
}: {
  row: AffectRow;
  full: AffectFulls;
  measure: ChipMeasure;
  chipH: number;
}) {
  const kind = chipKind(row);
  if (kind === 'missing') return <MissingChip row={row} measure={measure} chipH={chipH} />;
  const tone = row.tone;
  const chip = chipTone(row);
  // Only a tracked chip has a ground, so only it drains.
  const gauge = kind === 'tracked' ? gaugeFraction(row, full) : null;
  const hours = hoursOf(row);
  // The hours and the state show only as glyphs and color, so a screen
  // reader hears them as words after the name.
  const words = affectWords(row.state, row.ticks);
  const cls = [
    'pane-chip',
    `pane-chip-${kind}`,
    chip ? `is-${chip}` : '',
    gauge !== null && gauge < 1 ? 'is-draining' : '',
  ]
    .filter(Boolean)
    .join(' ');
  const style =
    gauge !== null && gauge < 1
      ? ({ ['--gauge' as string]: Number(gauge.toFixed(4)) } as CSSProperties)
      : undefined;
  return (
    <li className={cls} style={style}>
      <span className="pane-chip-name">
        {row.name}
        {words && <span className="pane-sr">{words}</span>}
      </span>
      {hours && (
        <span className={`pane-chip-hours${tone ? ` is-${tone}` : ''}`} aria-hidden="true">
          {hours}
        </span>
      )}
    </li>
  );
}

/** A tracked affect you are missing: the name in red and the game's
 *  `-`, on no ground, ringed in soft red dots. */
function MissingChip({
  row,
  measure,
  chipH,
}: {
  row: AffectRow;
  measure: ChipMeasure;
  chipH: number;
}) {
  const ref = useRef<HTMLLIElement | null>(null);
  const box = useBoxSize(ref);
  const hours = hoursOf(row);
  // The ring follows the chip's own width once it is measured, since a
  // long name ellipsizes and the chip narrows. Until then, and in a
  // test, it takes the width chipsGrid packed it at. Its height is a
  // chip's at your panel size.
  const width = box?.width ?? chipWidth(row.name, hours, measure);
  return (
    <li ref={ref} className="pane-chip pane-chip-missing">
      <span className="pane-chip-name">
        {row.name}
        <span className="pane-sr">{affectWords(row.state, row.ticks)}</span>
      </span>
      <span className="pane-chip-hours" aria-hidden="true">
        {hours}
      </span>
      <ChipDots width={width} height={chipH} />
    </li>
  );
}

/** The soft dotted ring round a missing chip: a zero length dash with
 *  a round cap draws each dot, so they come out round where a CSS
 *  dotted border draws squares. The gap is in pixels, so the ring needs
 *  no pathLength, and the dots start half a gap in, so the seam where
 *  the path closes falls between two dots. */
function ChipDots({ width, height }: { width: number; height: number }) {
  const { w, h, r } = chipDotsPath(width, height);
  const { gap } = chipDots(width, height);
  const inset = CHIP_DOT_PX / 2;
  return (
    <svg className="pane-chip-dots" aria-hidden="true" focusable="false">
      <rect
        x={inset}
        y={inset}
        width={round3(w)}
        height={round3(h)}
        rx={round3(r)}
        strokeDasharray={`0 ${round3(gap)}`}
        strokeDashoffset={round3(gap / 2)}
      />
    </svg>
  );
}

const round3 = (n: number) => Math.round(n * 1000) / 1000;
