import type { CSSProperties } from 'react';
import { bandRuns, type BandEnv } from '../../lib/bandCells';
import { contrast, parseHex } from '../../lib/color';
import type { Cell } from '../../lib/sgrCells';

// One row of terminal text inside the prompt card: a prompt line in the
// candidate box, or a preset's sample in the start list. Every character
// sits on its own cell, `cellW` apart, in the colors the renderer in use
// draws, so a mark under a value lines up with it whatever font draws a
// fallback glyph. The card sets this text in the terminal's face at 13 px
// on 17.5 px rows, as the boards do (src/lib/useCellWidth.ts).

/** A span of cells to mark: the selection token, or the warn ring of a
 *  run Vosh cannot read. */
export interface CellMark {
  from: number;
  to: number;
  warn: boolean;
}

interface CellLineProps {
  cells: Cell[];
  env: BandEnv;
  cellW: number;
  /** The most cells a row shows whole. A row past it ends on an
   *  ellipsis two cells short of it, where the boards' sample column cuts
   *  a line that does not fit: the ellipsis needs its cell and a hair
   *  more. */
  limit?: number;
  marks?: readonly CellMark[];
  className?: string;
  style?: CSSProperties;
}

/** The ratio under which a color reads too dim on the selection token, so
 *  a marked value takes the terminal's text color, as the two 60s the game
 *  draws in 256 color 240 do on P3. */
const MARKED_MIN_CONTRAST = 3;

function inMark(marks: readonly CellMark[], col: number): CellMark | null {
  return marks.find((m) => col >= m.from && col < m.to) ?? null;
}

export function CellLine({
  cells,
  env,
  cellW,
  limit,
  marks = [],
  className,
  style,
}: CellLineProps) {
  const total = cells.reduce((n, c) => n + (c.ch === '' ? 0 : c.width), 0);
  const max = limit ?? Number.POSITIVE_INFINITY;
  const clipped = total > max;
  const kept = clipped ? Math.max(0, max - 2) : max;
  const runs = bandRuns(cells, env, kept);
  const ground = parseHex(env.bg);
  const face = (color: string, col: number): string => {
    const mark = inMark(marks, col);
    if (!mark || mark.warn) return color;
    const rgb = parseHex(color);
    if (!rgb || !ground) return color;
    return contrast(rgb, ground) < MARKED_MIN_CONTRAST ? env.fg : color;
  };
  const shown = clipped ? kept + 1 : total;
  return (
    <div
      className={['pc-cells', className].filter(Boolean).join(' ')}
      style={{ width: shown * cellW, ...style }}
    >
      {marks.map((mark) => (
        <span
          key={`${mark.from}-${mark.to}`}
          aria-hidden="true"
          className={mark.warn ? 'pc-cells-warn' : 'pc-cells-token'}
          style={{ left: mark.from * cellW, width: (mark.to - mark.from) * cellW }}
        />
      ))}
      {runs.map((run) =>
        run.glyphs.map((glyph) =>
          glyph.ch.trim().length === 0 ? null : (
            <span
              key={glyph.col}
              className="pc-cells-glyph"
              style={{
                left: glyph.col * cellW,
                width: glyph.cols * cellW,
                color: face(run.look.color, glyph.col),
                fontWeight: run.look.bold ? 700 : 400,
                fontStyle: run.look.italic ? 'italic' : 'normal',
                background: run.look.background ?? undefined,
                textDecorationLine: run.look.decoration ?? undefined,
              }}
            >
              {glyph.ch}
            </span>
          ),
        ),
      )}
      {clipped && (
        <span className="pc-cells-glyph pc-cells-more" style={{ left: kept * cellW, width: cellW }}>
          …
        </span>
      )}
    </div>
  );
}
