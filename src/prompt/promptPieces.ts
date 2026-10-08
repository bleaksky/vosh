// The prompt card's pieces: which parts you can pick, where
// Left and Right go, how Option with them moves a part, where typing,
// Return and Insert value… put what they add, what the Color row checks
// and says, and where the marks sit on your prompt. Pure, so the
// components stay about layout.

import { textCells } from '../terminal/sgrCells';
import { toHex } from '../theme/color';
import { layoutPrompt, type PieceSpan, type RegionOnScreen } from './promptPointer';
import type {
  PromptColorChoice,
  PromptEditOp,
  PromptFormatChoice,
  PromptPiece,
  PromptUnderlineStyle,
  PromptWhen,
} from '../ipc/promptDesign';

// ---------------------------------------------------------------------
// Picking a part
// ---------------------------------------------------------------------

/** What the card points at: a part you picked, or a place between parts
 *  where the caret waits. Places count from 0 before the first piece to
 *  the number of pieces after the last. */
export interface Pointing {
  picked: number | null;
  caret: number | null;
}

/** No part picked and no caret. */
export const NOWHERE: Pointing = { picked: null, caret: null };

type PieceShape = Pick<PromptPiece, 'piece' | 'kind' | 'shows'>;

/** The parts you can pick, in order: every part that takes cells, each
 *  line break, which its ↵ stands for, and each push to the right edge,
 *  which its spaces stand for. */
export function pickable(pieces: readonly PieceShape[]): number[] {
  return pieces.filter((p) => p.shows || p.kind === 'nl' || p.kind === 'right').map((p) => p.piece);
}

/** Where Left (-1) or Right (1) goes: the part before or after the one
 *  you picked, or next to the caret, or the last or first part when
 *  nothing is picked. It stays at the first and the last part. */
export function step(pieces: readonly PieceShape[], from: Pointing, dir: -1 | 1): Pointing {
  const parts = pickable(pieces);
  if (parts.length === 0) return from;
  let next: number | undefined;
  if (from.picked !== null) {
    const at = parts.indexOf(from.picked);
    next = at < 0 ? parts[0] : parts[Math.max(0, Math.min(parts.length - 1, at + dir))];
  } else if (from.caret !== null) {
    const caret = from.caret;
    next = dir > 0 ? parts.find((p) => p >= caret) : [...parts].reverse().find((p) => p < caret);
    next ??= dir > 0 ? parts[parts.length - 1] : parts[0];
  } else {
    next = dir > 0 ? parts[0] : parts[parts.length - 1];
  }
  return { picked: next, caret: null };
}

/** The edit Option with Left (-1) or Right (1) makes: the part you picked
 *  moves past the part before or after it. None at either end. */
export function moveOp(
  pieces: readonly PieceShape[],
  picked: number | null,
  dir: -1 | 1,
): PromptEditOp | null {
  if (picked === null) return null;
  const parts = pickable(pieces);
  const at = parts.indexOf(picked);
  const past = at < 0 ? undefined : parts[at + dir];
  if (past === undefined) return null;
  return { op: 'move', piece: picked, to: dir < 0 ? past : past + 1 };
}

/** A move Option with Left or Right made: the design before and after it,
 *  where the part was and where it landed, the way it went, and whether
 *  the design followed the game before it. */
export interface MoveMade {
  before: string;
  after: string;
  from: number;
  landed: number;
  dir: -1 | 1;
  mirror: boolean;
}

/** The move the opposite key takes back, so Option with Right then
 *  Option with Left puts a part back exactly where it was, even when the
 *  move ran two texts together into one part. It is the newest move,
 *  while the design is still what it made, the part it moved is still
 *  picked, and the key goes the other way. */
export function moveBack(
  made: readonly MoveMade[],
  template: string,
  picked: number | null,
  dir: -1 | 1,
): MoveMade | null {
  const last = made[made.length - 1];
  if (!last || last.after !== template || last.landed !== picked || last.dir === dir) return null;
  return last;
}

/** The place right after the last part that shows, before the codes a
 *  design may end on, such as the `%c_reset` after your prompt's
 *  trailing space. A click past the end of your prompt puts the caret
 *  there. */
export function endPlace(pieces: readonly PieceShape[]): number {
  const parts = pickable(pieces);
  return parts.length > 0 ? parts[parts.length - 1] + 1 : 0;
}

/** Where what you add goes: at the caret, right after the part you
 *  picked, or at the end of your design. */
export function insertPlace(pieces: readonly PieceShape[], from: Pointing): number {
  if (from.caret !== null) return Math.min(from.caret, pieces.length);
  if (from.picked !== null) return Math.min(from.picked + 1, pieces.length);
  return endPlace(pieces);
}

/** The edits a value added at `place` makes. Added at the end of a
 *  design that ends in a space, as the game's prompt and every
 *  preset do, a space follows it, so your typed command stays a cell
 *  off it. */
export function insertOps(
  pieces: readonly Pick<PromptPiece, 'piece' | 'kind' | 'shows' | 'literal'>[],
  place: number,
  field: string,
  format?: PromptFormatChoice,
): PromptEditOp[] {
  const ops: PromptEditOp[] = [
    format
      ? { op: 'insert_field', at: place, field, format }
      : { op: 'insert_field', at: place, field },
  ];
  const parts = pickable(pieces);
  const last = parts.length > 0 ? pieces[parts[parts.length - 1]] : undefined;
  const endsInSpace = last?.kind === 'text' && (last.literal ?? '').endsWith(' ');
  if (place === endPlace(pieces) && endsInSpace) {
    ops.push({ op: 'insert_text', at: place + 1, text: ' ' });
  }
  return ops;
}

/** What Backspace (-1) or Delete (1) does: the part you picked goes, or
 *  with the caret placed, the character before or after it in text, or
 *  the whole part there. None with nothing to take. */
export function deleteOp(
  pieces: readonly Pick<PromptPiece, 'piece' | 'kind' | 'shows' | 'literal'>[],
  from: Pointing,
  dir: -1 | 1,
): PromptEditOp | null {
  if (from.picked !== null) return { op: 'remove', piece: from.picked };
  if (from.caret === null) return null;
  const parts = pickable(pieces);
  const caret = from.caret;
  const target =
    dir < 0 ? [...parts].reverse().find((p) => p < caret) : parts.find((p) => p >= caret);
  if (target === undefined) return null;
  const piece = pieces[target];
  const chars = Array.from(piece?.literal ?? '');
  if (piece?.kind === 'text' && chars.length > 1) {
    const text = (dir < 0 ? chars.slice(0, -1) : chars.slice(1)).join('');
    return { op: 'set_text', piece: target, text };
  }
  return { op: 'remove', piece: target };
}

/** Where the caret waits after a change: past the part the edit acted
 *  on, or where the removed part was. */
export function caretAfter(op: PromptEditOp, landed: number | null): number | null {
  if (landed !== null) return landed + 1;
  if (op.op === 'remove') return op.piece;
  return null;
}

// ---------------------------------------------------------------------
// The rows of a part
// ---------------------------------------------------------------------

/** The swatches of the Color row, in order: the terminal's
 *  text, By value, then the theme's red, green, yellow, blue, magenta,
 *  cyan and gray. */
export const THEME_SWATCHES: readonly { index: number; label: string }[] = [
  { index: 1, label: 'Theme red' },
  { index: 2, label: 'Theme green' },
  { index: 3, label: 'Theme yellow' },
  { index: 4, label: 'Theme blue' },
  { index: 5, label: 'Theme magenta' },
  { index: 6, label: 'Theme cyan' },
  { index: 8, label: 'Theme gray' },
];

/** Which swatch a color checks, or `custom` for a color no swatch names. */
export type Swatch = 'default' | 'by_value' | number | 'custom';

export function swatchOf(color: PromptColorChoice): Swatch {
  switch (color.kind) {
    case 'default':
      return 'default';
    case 'by_value':
      return color.field || color.game || color.steps ? 'custom' : 'by_value';
    case 'named':
      return THEME_SWATCHES.some((s) => s.index === color.index) ? color.index : 'custom';
    default:
      return 'custom';
  }
}

/** What the Custom field shows: the color as #rrggbb when it is one no
 *  swatch names, with `palette` giving a theme or 256 color its hex, and
 *  empty otherwise. */
export function customText(color: PromptColorChoice, palette: (index: number) => string): string {
  return swatchOf(color) === 'custom' ? colorHex(color, palette) : '';
}

/** What a color field says for a color by how full a value is, which no
 *  hex can show: By value for the part's own, By and the name for
 *  another value, By game for the game's own bands, and By steps for the
 *  eleven steps from red to green. Null for any other color. */
export function byValueName(color: PromptColorChoice): string | null {
  if (color.kind !== 'by_value') return null;
  if (color.game) return 'By game';
  if (color.steps) return color.field ? `By ${color.field} in steps` : 'By steps';
  return color.field ? `By ${color.field}` : 'By value';
}

/** A color as #rrggbb, a theme color through `palette` too, for a field
 *  that offers no swatches, as the underline color. Empty for the text's
 *  own color and for a color by how full a value is, which byValueName
 *  names. */
export function colorHex(color: PromptColorChoice, palette: (index: number) => string): string {
  switch (color.kind) {
    case 'rgb':
      return toHex(color);
    case 'index':
    case 'named':
      return palette(color.index);
    default:
      return '';
  }
}

/** The color a #rrggbb from the Custom field writes. */
export function customColor(hex: string): PromptColorChoice | null {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex.trim());
  if (!m) return null;
  return {
    kind: 'rgb',
    r: parseInt(m[1], 16),
    g: parseInt(m[2], 16),
    b: parseInt(m[3], 16),
  };
}

export const THEME_HINT =
  'Theme colors follow your theme. A custom color stays the same everywhere.';
export const BY_VALUE_HINT =
  "By value uses your theme's green, yellow, and red. It turns yellow below two thirds and red below one third.";
export const STEPS_HINT =
  'By steps runs from red to green in eleven fixed colors, one for each tenth of the value.';

/** The line under the Color row: By value's rule while the text or its
 *  ground follows how full a value is, its own or another's, and the
 *  steps' rule while either goes by steps. The game's own bands follow
 *  the game's rule instead. */
export function colorHint(
  color: PromptColorChoice,
  background: PromptColorChoice = { kind: 'default' },
): string {
  const byValue = (c: PromptColorChoice) => c.kind === 'by_value' && !c.game && !c.steps;
  const steps = (c: PromptColorChoice) => c.kind === 'by_value' && !c.game && c.steps === true;
  if (byValue(color) || byValue(background)) return BY_VALUE_HINT;
  return steps(color) || steps(background) ? STEPS_HINT : THEME_HINT;
}

/** The line under a line break's When row, or none while it shows
 *  always. */
export function breakHint(when: PromptWhen): string | null {
  if (when === 'fight') {
    return 'The line above shows only in a fight, so out of a fight your prompt is one line.';
  }
  if (when === 'not_fight') {
    return 'The line above shows only out of a fight, so in a fight your prompt is one line.';
  }
  return null;
}

/** The line under a When row that a condition outside another one holds. */
export const WHEN_FIXED_HINT =
  'Another part decides when this part shows. Change it in Edit as text.';

/** The rows a part shows: a value has Show as, a bar Width and no
 *  Style, text its words, and a line break or a push to the right edge
 *  only When. Every part that takes a color takes a Background, and the
 *  Underline row with its kind and color shows while an underline is
 *  on. */
export function rowsOf(piece: Pick<PromptPiece, 'kind' | 'format' | 'forms' | 'underline'>): {
  showAs: boolean;
  text: boolean;
  width: boolean;
  color: boolean;
  background: boolean;
  style: boolean;
  underline: boolean;
} {
  const value = piece.kind === 'value' || piece.kind === 'cur_max' || piece.kind === 'percent';
  const bar = piece.format === 'bar';
  const breaks = piece.kind === 'nl' || piece.kind === 'right';
  const style = !breaks && !bar;
  return {
    showAs: value && piece.forms.length > 0,
    text: piece.kind === 'text',
    width: bar,
    color: !breaks,
    background: !breaks,
    style,
    underline: style && piece.underline,
  };
}

/** The kinds of underline the Underline row offers, in order, each with the CSS line its segment draws. */
export const UNDERLINE_KINDS: readonly {
  style: PromptUnderlineStyle;
  label: string;
  line: 'solid' | 'double' | 'wavy' | 'dotted' | 'dashed';
}[] = [
  { style: 'underline', label: 'Single', line: 'solid' },
  { style: 'double', label: 'Double', line: 'double' },
  { style: 'curly', label: 'Curly', line: 'wavy' },
  { style: 'dotted', label: 'Dotted', line: 'dotted' },
  { style: 'dashed', label: 'Dashed', line: 'dashed' },
];

/** The styles the Style row's More styles menu holds, past B, I and U. */
export const MORE_STYLES: readonly {
  style: 'strike' | 'dim' | 'inverse' | 'blink';
  label: string;
}[] = [
  { style: 'strike', label: 'Strikethrough' },
  { style: 'dim', label: 'Dim' },
  { style: 'inverse', label: 'Reverse' },
  { style: 'blink', label: 'Blink' },
];

/** The underline kinds More styles lists after its styles, past the
 *  single line of U. Picking one turns the underline on in that kind,
 *  and picking the one that is on turns the underline off. */
export const MORE_UNDERLINES: readonly {
  style: Exclude<PromptUnderlineStyle, 'underline'>;
  label: string;
}[] = [
  { style: 'double', label: 'Double underline' },
  { style: 'curly', label: 'Curly underline' },
  { style: 'dotted', label: 'Dotted underline' },
  { style: 'dashed', label: 'Dashed underline' },
];

/** The look More styles reads. */
export type MoreStylesPiece = Pick<
  PromptPiece,
  'strike' | 'dim' | 'inverse' | 'blink' | 'underline_style'
>;

/** What the More styles button reads: the styles it holds that are on,
 *  then its underline kind, so you see them without opening it, or More
 *  styles with none on. */
export function moreLabel(piece: MoreStylesPiece): string {
  const on = [
    ...MORE_STYLES.filter((s) => piece[s.style]).map((s) => s.label),
    ...MORE_UNDERLINES.filter((k) => piece.underline_style === k.style).map((k) => k.label),
  ];
  if (on.length === 0) return 'More styles';
  return [on[0], ...on.slice(1).map((label) => label.toLowerCase())].join(', ');
}

// ---------------------------------------------------------------------
// Where the marks sit
// ---------------------------------------------------------------------

/** A box in client px. */
export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** The terminal's cell grid in client px. */
export interface Grid {
  left: number;
  top: number;
  cellW: number;
  cellH: number;
}

/** Where cells of a drawn prompt land on screen. Rows and columns count
 *  as the spans count them, the row from `%nl` and the cell in it before
 *  any wrap. */
export interface CellMapper {
  /** The boxes cells `col` to `col + width` of row `row` cover, one per
   *  screen row they land on. */
  boxes: (row: number, col: number, width: number) => Box[];
  /** The left edge and top of cell `col` of row `row`, or of the cell
   *  right after the row's last when `col` is past it. */
  point: (row: number, col: number) => { left: number; top: number } | null;
}

/** The mapper for the open row in the text: each row laid out from the
 *  region's start at the renderer's width, as the renderers wrap it. */
export function textMapper(plain: string, region: RegionOnScreen, grid: Grid): CellMapper {
  const layout = layoutPrompt(plain, region.col, region.cols);
  const top = (row: number) => grid.top + (region.row + row) * grid.cellH;
  const left = (col: number) => grid.left + col * grid.cellW;
  return {
    boxes(row, col, width) {
      const runs = new Map<number, { from: number; to: number }>();
      for (const p of layout[row] ?? []) {
        if (p.width === 0 || p.cell < col || p.cell >= col + width) continue;
        const run = runs.get(p.row);
        if (run) {
          run.from = Math.min(run.from, p.col);
          run.to = Math.max(run.to, p.col + p.width);
        } else {
          runs.set(p.row, { from: p.col, to: p.col + p.width });
        }
      }
      return [...runs.entries()]
        .sort((a, b) => a[0] - b[0])
        .map(([screenRow, run]) => ({
          left: left(run.from),
          top: top(screenRow),
          width: (run.to - run.from) * grid.cellW,
          height: grid.cellH,
        }));
    },
    point(row, col) {
      const placed = (layout[row] ?? []).filter((p) => p.width > 0);
      const at = placed.find((p) => p.cell === col);
      if (at) return { left: left(at.col), top: top(at.row) };
      const last = placed[placed.length - 1];
      if (last) return { left: left(last.col + last.width), top: top(last.row) };
      // An empty row: its first cell, the region's start for the first.
      const start = row === 0 ? region.col : 0;
      const screenRow = row === 0 ? 0 : (layout[row - 1]?.at(-1)?.row ?? row - 1) + 1;
      return { left: left(start), top: top(screenRow) };
    },
  };
}

/** The mapper for the pinned band: it never wraps, and puts each cell at
 *  its column from the text's left edge. `rowsTop` is the top of the
 *  band's first shown row and `first` the row of the design it shows
 *  first. */
export function dockMapper(input: {
  left: number;
  rowsTop: number;
  first: number;
  shown: number;
  cellW: number;
  cellH: number;
  rows: readonly string[];
}): CellMapper {
  const { left, rowsTop, first, shown, cellW, cellH, rows } = input;
  const top = (row: number) => rowsTop + (row - first) * cellH;
  const visible = (row: number) => row >= first && row < first + shown;
  return {
    boxes(row, col, width) {
      if (!visible(row) || width <= 0) return [];
      return [{ left: left + col * cellW, top: top(row), width: width * cellW, height: cellH }];
    },
    point(row, col) {
      if (!visible(row)) return null;
      const cells = textCells(rows[row] ?? '');
      return { left: left + Math.min(col, cells) * cellW, top: top(row) };
    },
  };
}

/** The marks the card draws over your prompt. */
export interface MarkLayout {
  /** The part you picked: the accent tint and a 1 px accent ring. */
  picked: Box[];
  /** Each value Vosh reads on the game's own line, or the whole line,
   *  while the card reads your codes: the accent tint alone. */
  values: Box[];
  /** Parts Vosh cannot fill: a 1 px warn ring. */
  warn: Box[];
  /** A ↵ after each row a line break ends, two cells wide. */
  returns: { piece: number; box: Box; picked: boolean }[];
  /** The caret, 2 by 17 in the accent color. */
  caret: Box | null;
}

/** The ↵ takes two cells after its row, the caret 2 px. */
const RETURN_CELLS = 2;
const CARET_W = 2;
const CARET_H = 17;

/** Lay out the marks for `spans`, the pieces of what your prompt shows,
 *  through `mapper`. `lineBreaks` are the line break pieces, whose spans
 *  take no cells and sit at the end of the row they end. */
export function layoutMarks(input: {
  spans: readonly PieceSpan[];
  lineBreaks: ReadonlySet<number>;
  mapper: CellMapper;
  grid: Pick<Grid, 'cellW' | 'cellH'>;
  pointing: Pointing;
  warn: ReadonlySet<number>;
}): MarkLayout {
  const { spans, lineBreaks, mapper, grid, pointing, warn } = input;
  const boxesOf = (piece: number) =>
    spans
      .filter((s) => s.piece === piece && s.width > 0)
      .flatMap((s) => mapper.boxes(s.row, s.col, s.width));
  const returns: MarkLayout['returns'] = [];
  for (const span of spans) {
    if (!lineBreaks.has(span.piece)) continue;
    const at = mapper.point(span.row, span.col);
    if (!at) continue;
    returns.push({
      piece: span.piece,
      box: { left: at.left, top: at.top, width: RETURN_CELLS * grid.cellW, height: grid.cellH },
      picked: pointing.picked === span.piece,
    });
  }
  let caret: Box | null = null;
  if (pointing.caret !== null) {
    const place = pointing.caret;
    // After the last cell of the part before the place, or before the
    // first cell of the part after it.
    const before = spans
      .filter((s) => s.piece < place && (s.width > 0 || lineBreaks.has(s.piece)))
      .sort((a, b) => a.piece - b.piece || a.row - b.row || a.col - b.col)
      .at(-1);
    const after = spans
      .filter((s) => s.piece >= place && s.width > 0)
      .sort((a, b) => a.piece - b.piece || a.row - b.row || a.col - b.col)[0];
    const at = before
      ? lineBreaks.has(before.piece)
        ? mapper.point(before.row + 1, 0)
        : mapper.point(before.row, before.col + before.width)
      : after
        ? mapper.point(after.row, after.col)
        : mapper.point(0, 0);
    if (at) {
      caret = {
        left: at.left,
        top: at.top + (grid.cellH - CARET_H) / 2,
        width: CARET_W,
        height: CARET_H,
      };
    }
  }
  return {
    picked:
      pointing.picked === null || lineBreaks.has(pointing.picked) ? [] : boxesOf(pointing.picked),
    values: [],
    warn: [...warn].flatMap(boxesOf),
    returns,
    caret,
  };
}

/** A run of cells on the game's own lines the open row shows while the
 *  card reads your codes: its row among those lines, its cells, and
 *  whether it carries the warn ring. */
export interface RawMark {
  row: number;
  col: number;
  width: number;
  warn: boolean;
}

/** The marks on the game's own lines: every line whole
 *  while you tell Vosh your prompt, else each value the newest read
 *  marks, on a line the row shows. A read whose lines differ from what
 *  the row shows marks nothing. Its marks count characters. */
export function rawMarks(
  open: { raw_lines: readonly string[]; raw_from: number },
  read: {
    plain: string;
    marks: readonly { line: number; start: number; end: number; warn: boolean }[];
  } | null,
  whole: boolean,
): RawMark[] {
  if (whole) {
    return open.raw_lines
      .map((line, row) => ({ row, col: 0, width: textCells(line), warn: false }))
      .filter((m) => m.width > 0);
  }
  if (!read) return [];
  const lines = read.plain.split('\n');
  const out: RawMark[] = [];
  for (const mark of read.marks) {
    const row = mark.line - open.raw_from;
    const line = lines[mark.line];
    if (line === undefined || open.raw_lines[row] !== line) continue;
    const chars = Array.from(line);
    const col = textCells(chars.slice(0, mark.start).join(''));
    const width = textCells(chars.slice(mark.start, mark.end).join(''));
    if (width > 0) out.push({ row, col, width, warn: mark.warn });
  }
  return out;
}

/** The marks on the game's own line through `mapper`: each value in the
 *  accent tint alone, and a run Vosh cannot read in the warn ring. */
export function rawLayout(raw: readonly RawMark[], mapper: CellMapper): MarkLayout {
  const boxes = (warn: boolean) =>
    raw.filter((m) => m.warn === warn).flatMap((m) => mapper.boxes(m.row, m.col, m.width));
  return { picked: [], values: boxes(false), warn: boxes(true), returns: [], caret: null };
}

/** What a reader hears as you pick a part of your prompt, which the card
 *  says in a live region: its name, then what it reads now or its words.
 *  Empty with no part picked. */
export function pickAnnouncement(piece: PromptPiece | null): string {
  if (!piece) return '';
  const detail = piece.meta ?? (piece.kind === 'text' ? piece.literal : null);
  return detail ? `${piece.label}, ${detail}` : piece.label;
}
