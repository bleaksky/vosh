import type { CSSProperties } from 'react';
import { bandRuns, decorationLine, type BandEnv } from '../terminal/bandCells';
import { contrast, parseHex } from '../theme/color';
import { sampleCut } from './cardRules';
import type { Cell } from '../terminal/sgrCells';

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
  /** A column the row is cut to as the boards' samples are, in px: whole
   *  when it fits, else ending on an ellipsis, and as wide as the column
   *  (see sampleCut). It takes the place of `limit`. */
  column?: number;
  marks?: readonly CellMark[];
  className?: string;
  style?: CSSProperties;
}

/** The ratio under which a color reads too dim on the selection token, so
 *  a marked value takes the selection text, as the two 60s the game draws
 *  in 256 color 240 do on P3. */
const MARKED_MIN_CONTRAST = 3;

function inMark(marks: readonly CellMark[], col: number): CellMark | null {
  return marks.find((m) => col >= m.from && col < m.to) ?? null;
}

const FULL_BLOCK = '\u2588';

type Glyph = { col: number; cols: number; ch: string };

/** A run's glyphs, with each stretch of full blocks side by side taken as
 *  one bar. A font can leave a hairline between two full blocks set on
 *  their own, which the terminal grid never shows. */
function glyphParts(glyphs: readonly Glyph[]): (Glyph & { bar: boolean })[] {
  const parts: (Glyph & { bar: boolean })[] = [];
  for (const glyph of glyphs) {
    const last = parts[parts.length - 1];
    const block = glyph.ch === FULL_BLOCK;
    if (block && last?.bar && last.col + last.cols === glyph.col) {
      last.cols += glyph.cols;
      last.ch += glyph.ch;
    } else {
      parts.push({ ...glyph, bar: block });
    }
  }
  return parts;
}

export function CellLine({
  cells,
  env,
  cellW,
  limit,
  column,
  marks = [],
  className,
  style,
}: CellLineProps) {
  const total = cells.reduce((n, c) => n + (c.ch === '' ? 0 : c.width), 0);
  const cut = column === undefined ? null : sampleCut(total, cellW, column);
  const max = cut ? cut.kept : (limit ?? Number.POSITIVE_INFINITY);
  const clipped = total > max;
  const kept = cut ? cut.kept : clipped ? Math.max(0, max - 2) : max;
  const runs = bandRuns(cells, env, kept);
  const ground = parseHex(env.selection);
  const face = (color: string, col: number): string => {
    const mark = inMark(marks, col);
    if (!mark || mark.warn) return color;
    const rgb = parseHex(color);
    if (!rgb || !ground) return color;
    return contrast(rgb, ground) < MARKED_MIN_CONTRAST ? env.selectionText : color;
  };
  const shown = clipped ? kept + 1 : total;
  // A reader hears the line as its text, as far as it shows: each glyph
  // is placed on its own, which reads one character at a time with the
  // spaces between words gone.
  let col = 0;
  let text = '';
  for (const cell of cells) {
    if (col >= kept) break;
    text += cell.ch;
    if (cell.ch !== '') col += cell.width;
  }
  text = text.trimEnd() + (clipped ? '…' : '');
  return (
    <div
      className={['pc-cells', className].filter(Boolean).join(' ')}
      style={{ width: cut ? cut.width : shown * cellW, ...style }}
    >
      <span className="visually-hidden">{text}</span>
      {marks.map((mark) => (
        <span
          key={`${mark.from}-${mark.to}`}
          aria-hidden="true"
          className={mark.warn ? 'pc-cells-warn' : 'pc-cells-token'}
          style={{ left: mark.from * cellW, width: (mark.to - mark.from) * cellW }}
        />
      ))}
      {runs.map((run) =>
        glyphParts(run.glyphs).map((glyph) => {
          if (glyph.ch.trim().length === 0) return null;
          const color = face(run.look.color, glyph.col);
          return (
            <span
              key={glyph.col}
              className={glyph.bar ? 'pc-cells-glyph pc-cells-block' : 'pc-cells-glyph'}
              aria-hidden="true"
              style={{
                left: glyph.col * cellW,
                width: glyph.cols * cellW,
                color,
                fontWeight: run.look.bold ? 700 : 400,
                fontStyle: run.look.italic ? 'italic' : 'normal',
                background: run.look.background ?? undefined,
                textDecorationLine: decorationLine(run.look),
              }}
            >
              {/* The bar's own color fills the font's height under its
                  blocks, so they read as one bar. */}
              {glyph.bar ? <span style={{ background: color }}>{glyph.ch}</span> : glyph.ch}
            </span>
          );
        }),
      )}
      {clipped && (
        <span
          className="pc-cells-glyph pc-cells-more"
          aria-hidden="true"
          style={{ left: kept * cellW, width: cellW }}
        >
          …
        </span>
      )}
    </div>
  );
}
