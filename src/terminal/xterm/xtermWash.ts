// Washed lines on xterm. The trigger engine marks a washed line with a
// quarter strength truecolor background, the wash tint of its mark
// (wash_tint in crates/automation/src/trigger/color.rs). That tint is a
// signal, not the color to show. The native renderer reads it and paints
// the row's field in the theme's colors instead (src-tauri/src/native/gpu/
// frame.rs). WashPainter rewrites the same signal in the bytes xterm
// takes, so a washed row reads the same on both renderers.
//
// The native rule decides which rows wash. A row whose first cell carries
// a signal and holds text is washed. Its signal cells and the cells left
// in the default ground take the field of that first cell's mark, out to
// the full width. A signal on any other row paints as the plain ground,
// and so does a row of signal colored blanks. xterm has no full width
// paint of its own, so each washed row ends with an erase to its end
// under the field.

/** For each wash tint, as #rrggbb, the field a washed row paints in. */
export type WashFields = ReadonlyMap<string, string>;

/** How far a washed row's field carries from the ground toward its mark,
 *  WASH_FIELD_MIX in the trigger crate. */
const WASH_FIELD_MIX = Math.fround(0.18);

/** The canonical xterm color of each mark in ANSI order, NamedColor::rgb
 *  in the trigger crate. Its wash tint is each channel divided by 4. */
const CANONICAL: readonly (readonly [number, number, number])[] = [
  [0x00, 0x00, 0x00],
  [0xcd, 0x00, 0x00],
  [0x00, 0xcd, 0x00],
  [0xcd, 0xcd, 0x00],
  [0x00, 0x00, 0xee],
  [0xcd, 0x00, 0xcd],
  [0x00, 0xcd, 0xcd],
  [0xe5, 0xe5, 0xe5],
  [0x7f, 0x7f, 0x7f],
  [0xff, 0x00, 0x00],
  [0x00, 0xff, 0x00],
  [0xff, 0xff, 0x00],
  [0x5c, 0x5c, 0xff],
  [0xff, 0x00, 0xff],
  [0x00, 0xff, 0xff],
  [0xff, 0xff, 0xff],
];

type Rgb = readonly [number, number, number];

function parseHex(color: string): Rgb {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})/i.exec(color);
  if (!m) return [0, 0, 0];
  return [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)];
}

function toHex(rgb: readonly number[]): string {
  return `#${rgb.map((c) => c.toString(16).padStart(2, '0')).join('')}`;
}

/** wash_field in the trigger crate, in the same f32 math. */
function washField(mark: Rgb, ground: Rgb): Rgb {
  const mix = (m: number, g: number) =>
    Math.round(Math.fround(g + Math.fround(Math.fround(m - g) * WASH_FIELD_MIX)));
  return [mix(mark[0], ground[0]), mix(mark[1], ground[1]), mix(mark[2], ground[2])];
}

/** The field of each wash tint, from the 16 colors the terminal draws
 *  in ANSI order and its ground, the way the native renderer finds them. */
export function washFields(ansi16: readonly string[], ground: string): WashFields {
  const g = parseHex(ground);
  return new Map(
    CANONICAL.map((c, i) => [
      toHex([c[0] >> 2, c[1] >> 2, c[2] >> 2]),
      toHex(washField(parseHex(ansi16[i] ?? '#000000'), g)),
    ]),
  );
}

/** Whether a pane fills anew from the scrollback when its wash fields
 *  go from `before` to `after`. xterm keeps the colors a row was written
 *  in, so a wash it painted since its last fill shows the old theme until
 *  it is written again. Nothing washed, or the same fields, keeps it. */
export function refillsWashes(before: WashFields, after: WashFields, washed: boolean): boolean {
  if (!washed) return false;
  if (before.size !== after.size) return true;
  for (const [tint, field] of before) if (after.get(tint) !== field) return true;
  return false;
}

/** The background SGR of a field. */
function fieldSgr(field: string): string {
  return `48;2;${parseHex(field).join(';')}`;
}

/** The SGR that puts the ground back. */
const GROUND = '49';

/** Where the escape at `i` ends, or -1 when `s` ends inside it. */
function escapeEnd(s: string, i: number): number {
  if (i + 1 >= s.length) return -1;
  const c = s[i + 1];
  if (c === '[') {
    let j = i + 2;
    while (j < s.length && s.charCodeAt(j) >= 0x20 && s.charCodeAt(j) <= 0x3f) j++;
    if (j >= s.length) return -1;
    const code = s.charCodeAt(j);
    return code >= 0x40 && code <= 0x7e ? j + 1 : j;
  }
  if (c === ']' || c === 'P' || c === '_' || c === '^' || c === 'X') {
    for (let j = i + 2; j < s.length; j++) {
      if (s[j] === '\x07') return j + 1;
      if (s[j] === '\x1b') return j + 1 < s.length ? (s[j + 1] === '\\' ? j + 2 : j) : -1;
    }
    return -1;
  }
  let j = i + 1;
  while (j < s.length && s.charCodeAt(j) >= 0x20 && s.charCodeAt(j) <= 0x2f) j++;
  return j < s.length ? j + 1 : -1;
}

/** The parameters of an SGR, or null when `tok` is not one. */
function sgrParams(tok: string): string | null {
  if (!tok.startsWith('\x1b[')) return null;
  const m = /^([0-9;:]*)m$/.exec(tok.slice(2));
  return m ? m[1] : null;
}

/** Whether `tok` erases cells in the background in force. */
function erases(tok: string): boolean {
  return tok.startsWith('\x1b[') && /^[0-9;]*[JKX]$/.test(tok.slice(2));
}

function printable(tok: string): boolean {
  if (tok.length === 0 || tok[0] === '\x1b') return false;
  const code = tok.charCodeAt(0);
  return code >= 0x20 && code !== 0x7f;
}

function isText(tok: string): boolean {
  return printable(tok) && tok !== ' ';
}

function rowEnds(tok: string): boolean {
  return tok === '\r' || tok === '\n';
}

/** One part of an SGR: the parameters it spans, and the wash tint it sets
 *  as #rrggbb when it is a truecolor background. */
interface SgrPart {
  items: string[];
  tint?: string;
}

/** Three color channels as #rrggbb, or '' when they are not three bytes. */
function tintOf(channels: string[]): string {
  const rgb = channels.map((c) => Number(c));
  const bytes = rgb.length === 3 && rgb.every((c) => Number.isInteger(c) && c >= 0 && c <= 255);
  return bytes ? toHex(rgb) : '';
}

/** The parts of an SGR's parameters. A 38, 48 or 58 without colons takes
 *  the numbers of its color after it, and one with colons is whole. */
function sgrParts(params: string): SgrPart[] {
  const items = params.split(';');
  const parts: SgrPart[] = [];
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    if (item.includes(':')) {
      const sub = item.split(':');
      const part: SgrPart = { items: [item] };
      if (sub[0] === '48' && sub[1] === '2') {
        part.tint = tintOf(sub.length >= 6 ? sub.slice(3, 6) : sub.slice(2, 5));
      }
      parts.push(part);
      continue;
    }
    const n = Number(item);
    if ((n === 38 || n === 48 || n === 58) && i + 1 < items.length) {
      const mode = Number(items[i + 1]);
      const span = mode === 2 ? 5 : mode === 5 ? 3 : 2;
      const taken = items.slice(i, i + span);
      const part: SgrPart = { items: taken };
      if (n === 48 && mode === 2) part.tint = tintOf(taken.slice(2));
      parts.push(part);
      i += span - 1;
      continue;
    }
    parts.push({ items: [item] });
  }
  return parts;
}

/** Paints the wash signal in the theme's colors, across outputs. It
 *  keeps back an escape an output splits, and the leading blanks of a
 *  row the signal opens until the row shows whether it holds text. */
export class WashPainter {
  // The input as it reads: the wash tint in force, the background in
  // force when none is, and whether inverse is on.
  private signal: string | null = null;
  private bg = GROUND;
  private inverse = false;
  // The background this painter last left xterm with.
  private outBg = GROUND;
  // The row: the tint of its first cell when that cell carries one, or
  // 'other' once it carries none, and whether it holds text.
  private rowFirst: string | 'other' | null = null;
  private rowText = false;
  // An escape an output split.
  private carry = '';
  // Tokens kept back until their row shows text or ends, and while they
  // replay, whether it did.
  private pending: string[] = [];
  private forced: boolean | null = null;
  private out = '';
  // The fields of the output in hand.
  private map: WashFields = new Map();

  /** `onWash` hears each washed row the painter paints. */
  constructor(
    private readonly fields: () => WashFields,
    private readonly onWash: () => void = () => {},
  ) {}

  /** Paint one output, keeping back what the next one completes. */
  paint(text: string): string {
    this.map = this.fields();
    const s = this.carry + text;
    this.carry = '';
    this.out = '';
    for (let i = 0; i < s.length; ) {
      let end = i + 1;
      if (s[i] === '\x1b') {
        end = escapeEnd(s, i);
        if (end < 0) {
          this.carry = s.slice(i);
          break;
        }
      }
      this.take(s.slice(i, end));
      i = end;
    }
    return this.out;
  }

  /** Paint a whole stream, with nothing kept back at its end. */
  static whole(text: string, fields: WashFields, onWash?: () => void): string {
    const painter = new WashPainter(() => fields, onWash);
    let out = painter.paint(text);
    if (painter.pending.length > 0) {
      painter.out = '';
      painter.resolve(false);
      out += painter.out;
    }
    return out + painter.carry;
  }

  /** Drop what was kept back, for a region written over whole. */
  drop(): void {
    this.carry = '';
    this.pending = [];
    this.rowFirst = null;
    this.rowText = false;
  }

  private take(tok: string): void {
    if (this.forced === null && this.pending.length > 0) {
      this.pending.push(tok);
      if (rowEnds(tok)) this.resolve(false);
      else if (isText(tok)) this.resolve(true);
      return;
    }
    if (this.forced === null && this.mayWash(tok)) {
      this.pending.push(tok);
      return;
    }
    this.emit(tok);
  }

  /** Whether `tok` paints differently once its row shows whether it
   *  holds text: a signal, or a blank or an erase under one, before the
   *  row's text on a row the signal may open. */
  private mayWash(tok: string): boolean {
    if (this.rowText || this.rowFirst === 'other' || isText(tok)) return false;
    const params = sgrParams(tok);
    if (params !== null) return sgrParts(params).some((p) => p.tint && this.isSignal(p.tint));
    return this.signal !== null && (printable(tok) || erases(tok));
  }

  private resolve(washed: boolean): void {
    const toks = this.pending;
    this.pending = [];
    this.forced = washed;
    for (const tok of toks) this.emit(tok);
    this.forced = null;
  }

  private isSignal(tint: string): boolean {
    return this.map.has(tint);
  }

  /** Whether the row in hand washes, as far as it has shown. */
  private washed(): boolean {
    if (this.rowFirst === 'other') return false;
    if (this.rowText) return this.rowFirst !== null;
    return this.forced === true;
  }

  /** The field of the row in hand, which only a washed row asks for. */
  private rowField(): string {
    this.onWash();
    const tint = this.rowFirst !== null && this.rowFirst !== 'other' ? this.rowFirst : this.signal;
    return fieldSgr(this.map.get(tint ?? '') ?? '#000000');
  }

  /** The background a cell takes here, or null when no wash touches it. */
  private wanted(): string | null {
    const washed = this.washed();
    if (this.signal !== null) return washed ? this.rowField() : GROUND;
    if (!washed) return null;
    return this.bg === GROUND && !this.inverse ? this.rowField() : this.bg;
  }

  private setBg(bg: string): void {
    if (bg !== this.outBg) this.out += `\x1b[${bg}m`;
    this.outBg = bg;
  }

  private emit(tok: string): void {
    if (rowEnds(tok)) {
      if (this.rowText && this.washed()) {
        this.setBg(this.rowField());
        this.out += '\x1b[K';
        if (this.signal === null) this.setBg(this.bg);
      }
      this.rowFirst = null;
      this.rowText = false;
      this.out += tok;
      return;
    }
    const params = sgrParams(tok);
    if (params !== null) {
      this.out += this.sgr(params, tok);
      return;
    }
    if (printable(tok) || erases(tok)) {
      if (this.rowFirst === null) this.rowFirst = this.signal ?? 'other';
      if (isText(tok)) this.rowText = true;
      const bg = this.wanted();
      if (bg !== null) this.setBg(bg);
    }
    this.out += tok;
  }

  /** Read an SGR, and write it with each signal in it painted. */
  private sgr(params: string, tok: string): string {
    let painted = false;
    const items: string[] = [];
    for (const part of sgrParts(params)) {
      const code = part.items[0];
      const n = code === '' ? 0 : Number(code);
      if (part.tint && this.isSignal(part.tint)) {
        this.signal = part.tint;
        const bg = this.washed() ? this.rowField() : GROUND;
        items.push(bg);
        this.outBg = bg;
        painted = true;
        continue;
      }
      items.push(...part.items);
      if (part.items.length === 1 && !code.includes(':') && n === 0) {
        this.signal = null;
        this.bg = GROUND;
        this.inverse = false;
        this.outBg = GROUND;
      } else if (n === 7 && part.items.length === 1) {
        this.inverse = true;
      } else if (n === 27 && part.items.length === 1) {
        this.inverse = false;
      } else if (
        n === 48 ||
        code.startsWith('48:') ||
        (n >= 40 && n <= 49) ||
        (n >= 100 && n <= 107)
      ) {
        const bg = part.items.join(';');
        this.signal = null;
        this.bg = bg;
        this.outBg = bg;
      }
    }
    return painted ? `\x1b[${items.join(';')}m` : tok;
  }
}
