// The prompt the session pins above the command line, read into terminal
// cells the way the terminal reads it: one row per line, one cell per
// column, each with every SGR attribute a MUD or your design can set. The
// band lays the cells out on the terminal's own cell grid and resolves
// their colors the way the active renderer does, so a pinned prompt looks
// as it would in the text.

/** A color as SGR sets it. `named` is 30 to 37 and 90 to 97 (and their
 *  backgrounds), which bold can brighten. `indexed` is 38;5;n. */
export type CellColor =
  | { kind: 'named'; n: number }
  | { kind: 'indexed'; n: number }
  | { kind: 'rgb'; r: number; g: number; b: number };

/** 0 none, 1 single, 2 double, 3 curly, 4 dotted, 5 dashed. */
export type UnderlineStyle = 0 | 1 | 2 | 3 | 4 | 5;

export interface CellAttrs {
  fg: CellColor | null;
  bg: CellColor | null;
  bold: boolean;
  dim: boolean;
  italic: boolean;
  underline: UnderlineStyle;
  underlineColor: CellColor | null;
  strike: boolean;
  inverse: boolean;
  hidden: boolean;
  /** SGR 5. The rapid blink of SGR 6 draws steady, as xterm draws it. */
  blink: boolean;
}

export interface Cell {
  /** The character, or a space for a blank the text skipped over. */
  ch: string;
  /** Columns it takes: 2 for a wide character. */
  width: 1 | 2;
  attrs: CellAttrs;
}

export const PLAIN: Readonly<CellAttrs> = Object.freeze({
  fg: null,
  bg: null,
  bold: false,
  dim: false,
  italic: false,
  underline: 0,
  underlineColor: null,
  strike: false,
  inverse: false,
  hidden: false,
  blink: false,
});

/** True for a character that takes two columns: CJK, Hangul, fullwidth
 *  forms and pictographs, as a Unicode 11 terminal counts them. */
export function isWide(code: number): boolean {
  return (
    (code >= 0x1100 && code <= 0x115f) ||
    (code >= 0x2e80 && code <= 0x303e) ||
    (code >= 0x3041 && code <= 0x33ff) ||
    (code >= 0x3400 && code <= 0x4dbf) ||
    (code >= 0x4e00 && code <= 0x9fff) ||
    (code >= 0xa000 && code <= 0xa4cf) ||
    (code >= 0xac00 && code <= 0xd7a3) ||
    (code >= 0xf900 && code <= 0xfaff) ||
    (code >= 0xfe30 && code <= 0xfe4f) ||
    (code >= 0xff00 && code <= 0xff60) ||
    (code >= 0xffe0 && code <= 0xffe6) ||
    (code >= 0x1f300 && code <= 0x1f64f) ||
    (code >= 0x1f900 && code <= 0x1f9ff) ||
    (code >= 0x20000 && code <= 0x3fffd)
  );
}

/** True for a combining mark, which joins the cell before it. */
export function isMark(glyph: string): boolean {
  return /\p{M}/u.test(glyph);
}

/** The cells code point `code` takes in a row: none for a combining mark,
 *  two for a wide character, one for any other. The spans of a rendered
 *  prompt count columns the same way (`vosh_prompt::wrap::cell_width`). */
export function cellWidth(code: number): 0 | 1 | 2 {
  if (code < 0x80) return 1;
  if (isMark(String.fromCodePoint(code))) return 0;
  return isWide(code) ? 2 : 1;
}

/** Read an extended color after 38, 48 or 58: `5;n` or `2;r;g;b`, with
 *  semicolons or colons. Returns the color and how many parameters it
 *  took after the 38. */
function extendedColor(params: number[], at: number): [CellColor | null, number] {
  const mode = params[at];
  if (mode === 5) {
    const n = params[at + 1];
    return [n === undefined ? null : { kind: 'indexed', n: n & 255 }, 2];
  }
  if (mode === 2) {
    // A colon form may carry an empty color space id first.
    const r = params[at + 1];
    const g = params[at + 2];
    const b = params[at + 3];
    if (r === undefined || g === undefined || b === undefined) return [null, 4];
    return [{ kind: 'rgb', r: r & 255, g: g & 255, b: b & 255 }, 4];
  }
  return [null, 1];
}

/** Apply one SGR sequence's parameters to `attrs`. `sub` gives the colon
 *  separated sub parameters of each, for 4:x underline styles. */
export function applySgr(prev: CellAttrs, params: number[], sub: number[][]): CellAttrs {
  const a: CellAttrs = { ...prev };
  if (params.length === 0) return { ...PLAIN };
  let i = 0;
  while (i < params.length) {
    const p = params[i];
    const subs = sub[i] ?? [];
    switch (true) {
      case p === 0:
        Object.assign(a, PLAIN);
        break;
      case p === 1:
        a.bold = true;
        break;
      case p === 2:
        a.dim = true;
        break;
      case p === 3:
        a.italic = true;
        break;
      case p === 4: {
        const style = subs.length > 0 ? subs[0] : 1;
        a.underline = (style >= 0 && style <= 5 ? style : 1) as UnderlineStyle;
        break;
      }
      case p === 5:
        a.blink = true;
        break;
      case p === 7:
        a.inverse = true;
        break;
      case p === 8:
        a.hidden = true;
        break;
      case p === 9:
        a.strike = true;
        break;
      case p === 21:
        a.underline = 2;
        break;
      case p === 22:
        a.bold = false;
        a.dim = false;
        break;
      case p === 23:
        a.italic = false;
        break;
      case p === 24:
        a.underline = 0;
        break;
      case p === 25:
        a.blink = false;
        break;
      case p === 27:
        a.inverse = false;
        break;
      case p === 28:
        a.hidden = false;
        break;
      case p === 29:
        a.strike = false;
        break;
      case p >= 30 && p <= 37:
        a.fg = { kind: 'named', n: p - 30 };
        break;
      case p === 39:
        a.fg = null;
        break;
      case p >= 40 && p <= 47:
        a.bg = { kind: 'named', n: p - 40 };
        break;
      case p === 49:
        a.bg = null;
        break;
      case p === 59:
        a.underlineColor = null;
        break;
      case p >= 90 && p <= 97:
        a.fg = { kind: 'named', n: p - 90 + 8 };
        break;
      case p >= 100 && p <= 107:
        a.bg = { kind: 'named', n: p - 100 + 8 };
        break;
      case p === 38 || p === 48 || p === 58: {
        // Colon form: the whole color rides in this parameter's subs.
        const [color, used] =
          subs.length > 0 ? extendedColor(stripColorSpace(subs), 0) : extendedColor(params, i + 1);
        if (p === 38) a.fg = color;
        else if (p === 48) a.bg = color;
        else a.underlineColor = color;
        if (subs.length === 0) i += used;
        break;
      }
      default:
        break;
    }
    i += 1;
  }
  return a;
}

/** `2::r:g:b` has an empty color space id before the channels. */
function stripColorSpace(subs: number[]): number[] {
  if (subs[0] === 2 && subs.length === 5) return [2, subs[2], subs[3], subs[4]];
  return subs;
}

/** Read `text` into rows of cells. `\r\n` and `\n` start a row, `\r` goes
 *  back to the start of the row, SGR sets attributes, and any other
 *  escape sequence takes no room. Tabs move to the next multiple of 8. */
export function parseSgrCells(text: string): Cell[][] {
  const rows: Cell[][] = [[]];
  let attrs: CellAttrs = { ...PLAIN };
  let col = 0;
  const row = () => rows[rows.length - 1];
  const put = (ch: string, width: 1 | 2) => {
    const cells = row();
    while (cells.length < col) cells.push({ ch: ' ', width: 1, attrs: { ...PLAIN } });
    cells[col] = { ch, width, attrs };
    if (width === 2) cells[col + 1] = { ch: '', width: 1, attrs };
    col += width;
  };
  let i = 0;
  while (i < text.length) {
    const ch = text[i];
    if (ch === '\x1b') {
      const next = text[i + 1];
      if (next === '[') {
        let j = i + 2;
        while (j < text.length && !/[\x40-\x7e]/.test(text[j])) j++;
        const final = text[j];
        if (final === 'm') {
          const body = text.slice(i + 2, j);
          const params: number[] = [];
          const sub: number[][] = [];
          for (const part of body.length > 0 ? body.split(';') : []) {
            const pieces = part.split(':').map((x) => (x === '' ? 0 : Number(x)));
            params.push(pieces[0] ?? 0);
            sub.push(pieces.slice(1));
          }
          attrs = applySgr(attrs, params, sub);
        }
        i = j + 1;
        continue;
      }
      if (next === ']') {
        let j = i + 2;
        while (
          j < text.length &&
          text[j] !== '\x07' &&
          !(text[j] === '\x1b' && text[j + 1] === '\\')
        ) {
          j++;
        }
        i = text[j] === '\x07' ? j + 1 : j + 2;
        continue;
      }
      i += 2;
      continue;
    }
    if (ch === '\n') {
      rows.push([]);
      col = 0;
      i += 1;
      continue;
    }
    if (ch === '\r') {
      col = 0;
      i += 1;
      continue;
    }
    if (ch === '\t') {
      const to = (Math.floor(col / 8) + 1) * 8;
      while (col < to) put(' ', 1);
      i += 1;
      continue;
    }
    const code = text.codePointAt(i) ?? 0;
    const glyph = String.fromCodePoint(code);
    i += glyph.length;
    if (code < 0x20 || code === 0x7f) continue;
    // A combining mark joins the cell before it.
    if (isMark(glyph)) {
      const cells = row();
      const last = cells[col - 1];
      if (last) last.ch += glyph;
      continue;
    }
    put(glyph, isWide(code) ? 2 : 1);
  }
  return rows;
}

/** The columns a row shows: up to its last cell that draws anything, a
 *  glyph or a background. A trailing space the prompt ends in does not
 *  count, so the band stops 4 px after the last glyph. */
export function shownColumns(row: Cell[]): number {
  for (let c = row.length - 1; c >= 0; c--) {
    const cell = row[c];
    if (cell.ch.trim().length > 0 || cell.attrs.bg !== null || cell.attrs.inverse) {
      return c + cell.width;
    }
  }
  return 0;
}
