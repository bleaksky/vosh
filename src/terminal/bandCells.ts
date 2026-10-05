// How a cell of the pinned prompt band looks: its colors, weight and
// lines, resolved the way the renderer in use draws the same cell in the
// text. xterm draws bold 30 to 37 in their bright colors and dim at half
// strength. The native grid brightens bold named colors too, darkens dim
// to 60 percent in linear light, and draws bright colors in the bold face
// only while Bright bold is on.

import { fromLinear, toHex, toLinear, toRgba, type Rgb } from '../theme/color';
import type { Cell, CellAttrs, CellColor } from './sgrCells';

export interface BandEnv {
  /** The 16 ANSI colors in use, in ANSI order, as #rrggbb. */
  palette: readonly string[];
  /** The terminal's text and ground colors. */
  fg: string;
  bg: string;
  /** The selection token and the text drawn on it. */
  selection: string;
  selectionText: string;
  renderer: 'xterm' | 'native';
  /** Bright bold, which only the native grid follows. */
  brightBold: boolean;
}

export interface BandCellLook {
  color: string;
  /** A ground of its own, drawn over the band. Null keeps the band. */
  background: string | null;
  bold: boolean;
  italic: boolean;
  /** The underline, or null: its CSS line style, and its own color, or
   *  null for the text color. */
  underline: { style: UnderlineLine; color: string | null } | null;
  /** A strikethrough, which every renderer draws as one straight line in
   *  the text color, whatever the underline's kind and color. */
  strike: boolean;
  /** SGR 5. The pinned band hides the glyph and both lines in the
   *  hidden half, as xterm and the native grid do. */
  blink: boolean;
}

/** A CSS text-decoration-style an underline draws in. */
export type UnderlineLine = 'solid' | 'double' | 'wavy' | 'dotted' | 'dashed';

/** The CSS text-decoration-line of a look's lines together, for a view
 *  that draws them plain, or undefined with none. */
export function decorationLine(look: BandCellLook): string | undefined {
  const lines = [look.underline && 'underline', look.strike && 'line-through'].filter(Boolean);
  return lines.length > 0 ? lines.join(' ') : undefined;
}

/** The color of the first six hex digits, black without them. */
function parseHex(hex: string): Rgb {
  const m = /^#?([0-9a-f]{6})/i.exec(hex.trim());
  if (!m) return { r: 0, g: 0, b: 0 };
  const n = parseInt(m[1], 16);
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
}

/** xterm's 256 color table past the 16: the 6x6x6 cube, then 24 grays. */
export function indexedRgb(n: number, palette: readonly string[]): Rgb {
  if (n < 16) return parseHex(palette[n] ?? '#000000');
  if (n < 232) {
    const i = n - 16;
    const v = (c: number) => (c === 0 ? 0 : 55 + c * 40);
    return { r: v(Math.floor(i / 36)), g: v(Math.floor((i % 36) / 6)), b: v(i % 6) };
  }
  const gray = 8 + (n - 232) * 10;
  return { r: gray, g: gray, b: gray };
}

function colorRgb(color: CellColor, palette: readonly string[]): Rgb {
  switch (color.kind) {
    case 'named':
    case 'indexed':
      return indexedRgb(color.n, palette);
    case 'rgb':
      return { r: color.r, g: color.g, b: color.b };
  }
}

/** A bright ANSI color, 8 to 15, named or indexed. */
function isBright(color: CellColor | null): boolean {
  return color !== null && color.kind !== 'rgb' && color.n >= 8 && color.n <= 15;
}

/** Bold brightens 30 to 37 to 90 to 97 on both renderers. */
function brightened(color: CellColor | null, bold: boolean): CellColor | null {
  if (bold && color?.kind === 'named' && color.n < 8) return { kind: 'named', n: color.n + 8 };
  return color;
}

const UNDERLINE_STYLES: readonly UnderlineLine[] = [
  'solid',
  'solid',
  'double',
  'wavy',
  'dotted',
  'dashed',
];

export function resolveCell(attrs: CellAttrs, env: BandEnv): BandCellLook {
  const fgColor = brightened(attrs.fg, attrs.bold);
  let fg: Rgb = fgColor ? colorRgb(fgColor, env.palette) : parseHex(env.fg);
  if (attrs.dim && env.renderer === 'native') {
    const dimmed = (c: number) => fromLinear(toLinear(c) * 0.6);
    fg = { r: dimmed(fg.r), g: dimmed(fg.g), b: dimmed(fg.b) };
  }
  const bgRgb: Rgb | null = attrs.bg ? colorRgb(attrs.bg, env.palette) : null;
  let color: string;
  let background: string | null;
  if (attrs.inverse) {
    color = toHex(bgRgb ?? parseHex(env.bg));
    background = toHex(fg);
  } else {
    color = toHex(fg);
    background = bgRgb ? toHex(bgRgb) : null;
  }
  if (attrs.dim && env.renderer === 'xterm') color = toRgba(parseHex(color), 0.5);
  // xterm hides concealed text. The native grid draws it.
  if (attrs.hidden && env.renderer === 'xterm') color = 'transparent';
  const bold = env.renderer === 'native' && isBright(fgColor) ? env.brightBold : attrs.bold;
  // The native grid draws one solid underline in the text color.
  const styled = env.renderer === 'xterm';
  return {
    color,
    background,
    bold,
    italic: attrs.italic,
    underline:
      attrs.underline > 0
        ? {
            style: styled ? UNDERLINE_STYLES[attrs.underline] : 'solid',
            color:
              styled && attrs.underlineColor
                ? toHex(colorRgb(attrs.underlineColor, env.palette))
                : null,
          }
        : null,
    strike: attrs.strike,
    blink: attrs.blink,
  };
}

/** One row of the band as runs of cells that look alike, each with the
 *  column it starts at and how many columns it takes. */
export interface BandRun {
  col: number;
  cols: number;
  text: string;
  look: BandCellLook;
  /** Each character on its own, with its column, so glyphs a fallback
   *  font draws wider or narrower never shift the ones after them. */
  glyphs: { col: number; cols: number; ch: string }[];
}

export function bandRuns(row: Cell[], env: BandEnv, limit: number): BandRun[] {
  const runs: BandRun[] = [];
  let col = 0;
  for (const cell of row) {
    if (col >= limit) break;
    if (cell.ch === '') {
      col += 1;
      continue;
    }
    const look = resolveCell(cell.attrs, env);
    const last = runs[runs.length - 1];
    const same =
      last !== undefined &&
      last.col + last.cols === col &&
      JSON.stringify(last.look) === JSON.stringify(look);
    const glyph = { col, cols: cell.width, ch: cell.ch };
    if (same) {
      last.cols += cell.width;
      last.text += cell.ch;
      last.glyphs.push(glyph);
    } else {
      runs.push({ col, cols: cell.width, text: cell.ch, look, glyphs: [glyph] });
    }
    col += cell.width;
  }
  return runs;
}
