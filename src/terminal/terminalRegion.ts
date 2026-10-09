// Marked regions in xterm.
//
// The session marks where a region it may replace later starts, with the
// private mark ESC ] 7717 ; o ; G BEL. A drawn prompt, a partial line
// painted at the end of a read, and a repaint each start one. A later
// output replaces a region: xterm finds it in its own buffer, checks that
// nothing was written after it, and only then erases from its start and
// writes the new text. The session never counts rows, since your typed
// echo reaches xterm before the session hears of it, and xterm wraps at
// its own width. The native grid follows the same rules in
// src-tauri/src/native/grid/regions.rs.
//
// Every write to one xterm goes through one RegionWriter, in order. A
// plain write passes straight to xterm while nothing waits. A write that
// reads the buffer waits for xterm to parse what came before it, reads
// the marker and the cursor, writes, and only then lets the queue go on.
//
// While your prompt shows pinned, an output can end on line ends it asks
// the writer to hold back, so the text ends on its last line while you
// wait. The writer writes them before the next thing that lands at the
// cursor: session text, a fresh replace, or a local write such as your
// echo. A replace of the open region lies before them, so it goes first.
// An output that writes nothing at the cursor keeps the longer of its
// hold and the one waiting, so hidden pulses never stack empty rows.
// The row the pinned prompt held is still where the next thing lands, so
// the line end that would end it, the first thing a framed echo or an
// error notice writes, writes nothing (closePinRow). Each output says
// whether that row is open after it.
//
// The wait is a write of nothing with a callback. It has to be an empty
// byte array, never an empty string. xterm 6.1 parses everything it
// holds before a resize, and that loop stops at the first empty string,
// then drops every write and callback after it. The writer would wait
// for good and xterm would show nothing more. The same loop can parse
// again what xterm had already parsed, callbacks included, so each wait
// runs its callback once.
//
// That loop starts from the first write xterm still keeps, and xterm
// keeps the writes it parsed in an earlier slice until it has parsed
// them all. A resize between two slices of a long backlog showed the
// backlog twice. So every resize goes through the writer too. Each write
// counts until xterm calls back for it. Once xterm called back for one
// while others are left, it is between two slices, and a resize waits
// until xterm has parsed them all. The writes that come meanwhile wait
// behind it, and a later resize takes its place. Otherwise the resize
// goes through at once, in the same task, as a drag of a divider and the
// rows the pinned band lends need. xterm has parsed none of what it
// holds then, so its flush parses each write once.
//
// With a mark picked, your echo, typed or from a quick key,
// starts with that mark. Where the echo lands decides the mark, so it
// waits for xterm to parse what came before it, held line ends and a
// restore included. When the row it lands on already ends in `>` before
// the cursor, as the game's own prompt does, the mark drops.

import { DEFAULT_ECHO_MARK } from '../input/maskedInput';

/** The private OSC a region mark uses. */
export const REGION_OSC = 7717;

/** The parts of xterm the writer uses. */
export interface RegionTerminal {
  readonly cols: number;
  readonly buffer: {
    readonly active: {
      readonly cursorX: number;
      readonly cursorY: number;
      readonly baseY: number;
      getLine(y: number):
        | {
            readonly isWrapped: boolean;
            translateToString(
              trimRight?: boolean,
              startColumn?: number,
              endColumn?: number,
            ): string;
          }
        | undefined;
    };
  };
  readonly parser: {
    registerOscHandler(
      ident: number,
      callback: (data: string) => boolean | Promise<boolean>,
    ): { dispose(): void };
  };
  write(data: string | Uint8Array, callback?: () => void): void;
  resize(cols: number, rows: number): void;
}

/** Replace region `gen` with `text`. When the region is closed, a
 *  `fresh` replace writes `text` on a new row and any other is dropped.
 *  `above` carries the lines the region's prompt shows right above it,
 *  such as a tank line shown as sent: when the rows right above the open
 *  region show `plain`, the writer erases from their first row and writes
 *  its `text` there instead. `tail` is the end of the region `text` leaves
 *  out, since the writer still holds it back as line ends that wait: a
 *  run of repeated lines the session rewrites while your prompt shows
 *  pinned. Written on a new row, or over an open region with nothing held
 *  back, as one loaded from your scrollback, the writer holds `tail` back
 *  in their place. */
export interface RegionReplace {
  gen: number;
  text: string;
  fresh: boolean;
  above?: { plain: string; text: string };
  tail?: string;
}

/** True when rows reading `rows`, top first, show the lines `plain`
 *  holds, however xterm or the word wrap broke them. Blanks do not count,
 *  since a word wrap drops the one it breaks at. The same rule as
 *  shows_lines in crates/prompt/src/stage/output.rs. */
export function showsLines(rows: string[], plain: string): boolean {
  const want = squeeze(plain);
  return want.length > 0 && squeeze(rows.join('')) === want;
}

function squeeze(text: string): string {
  return text.replace(/\s+/g, '');
}

/** True when a row that reads `before` up to the cursor already asks for
 *  your input, as a game's prompt such as `Account name> ` does: it ends
 *  in `>` once trailing blanks go. Your echo drops its mark there. The
 *  same rule as ends_in_prompt in src-tauri/src/native/grid/regions.rs. */
export function endsInPrompt(before: string): boolean {
  return before.trimEnd().endsWith('>');
}

/** The most rows the lines above a region are looked for in. */
const ABOVE_ROWS = 64;

/** One session output, decoded and wrapped: the replace goes first, then
 *  the text. `restore` is the live render for the region the output
 *  leaves open, which goes back over it before anything else lands.
 *  `hold` is line ends that wait for the next write. */
export interface RegionOutput {
  text: string;
  replace?: RegionReplace;
  restore?: string;
  hold?: string;
  /** Whether a pinned prompt's row is open after this output. Absent,
   *  whatever lands closes it. */
  pinRow?: boolean;
  /** The text starts a row of its own, as a line Vosh prints about
   *  itself does: a line end goes first when the cursor sits past the
   *  start of a row and no held line ends come first. The same rule as
   *  `Output::fresh` in crates/prompt/src/stage/output.rs. */
  fresh?: boolean;
}

/** What `text` does to the row a pinned prompt left open. The prompt is
 *  not in the text, so the line end that would end its row writes
 *  nothing: the first one, when only escape sequences and carriage
 *  returns come before it. `closed` says the row is closed, by that line
 *  end or by anything else that lands first. Escape sequences alone leave
 *  it open. The same rule as close_pin_row in
 *  crates/prompt/src/stage/output.rs. */
export function closePinRow(text: string): { text: string; closed: boolean } {
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === '\x1b') {
      i = escapeEnd(text, i);
    } else if (c === '\r') {
      i++;
    } else if (c === '\n') {
      const start = i > 0 && text[i - 1] === '\r' ? i - 1 : i;
      return { text: text.slice(0, start) + text.slice(i + 1), closed: true };
    } else {
      return { text, closed: true };
    }
  }
  return { text, closed: false };
}

/** Where the escape sequence that starts at `at` ends: after a CSI's
 *  final byte, after an OSC's BEL or ST, after the next character
 *  otherwise. */
function escapeEnd(text: string, at: number): number {
  const kind = text[at + 1];
  if (kind === '[') {
    let i = at + 2;
    while (i < text.length && !(text.charCodeAt(i) >= 0x40 && text.charCodeAt(i) <= 0x7e)) i++;
    return Math.min(i + 1, text.length);
  }
  if (kind === ']') {
    for (let i = at + 2; i < text.length; i++) {
      if (text[i] === '\x07') return i + 1;
      if (text[i] === '\x1b' && text[i + 1] === '\\') return i + 2;
    }
    return text.length;
  }
  return kind === undefined ? text.length : at + 2;
}

/** A write of nothing that xterm's flush before a resize still parses
 *  and calls back for, since it is not falsy as an empty string is. */
const NOTHING = new Uint8Array(0);

// eslint-disable-next-line no-control-regex
const MARK = /\x1b\]7717;o;(\d+)\x07/g;

/** The generation of the last region mark in `text`, or null. */
export function lastMark(text: string): number | null {
  let gen: number | null = null;
  for (const match of text.matchAll(MARK)) gen = Number(match[1]);
  return gen;
}

/** The line ends in `text` after its last region mark, which end the
 *  lines the region spans. */
export function breaksAfterMark(text: string): number {
  let after = -1;
  for (const match of text.matchAll(MARK)) after = (match.index ?? 0) + match[0].length;
  return after < 0 ? 0 : (text.slice(after).match(/\n/g) ?? []).length;
}

/** The escape that goes from the cursor at the end of a region back to
 *  its start, `above` rows up at column `col`, then erases to the end of
 *  the screen (rule c) with the default background. The cells it clears
 *  take the background in force, and a line the region ends on can leave
 *  its own on while the line ends after it wait. What the replace writes
 *  sets its own. The same as erase_back in
 *  src-tauri/src/native/grid/regions.rs. */
export function eraseBack(above: number, col: number): string {
  return `\r${above > 0 ? `\x1b[${above}A` : ''}${col > 0 ? `\x1b[${col}C` : ''}\x1b[49m\x1b[0J`;
}

/** Where the last mark xterm parsed came, counted in its line, since a
 *  line is what a resize keeps. xterm's markers can drift after a resize
 *  narrows a full scrollback, as its reflow moves them by the rows it
 *  added only to lines it kept and then by every row it trimmed. `rowsIn`
 *  counts the rows of that line above the row the mark came on, and
 *  `col` is its column there. `held` says the cursor sat past the last
 *  column, so the region starts at the next row. `cells` counts the
 *  cells from the start of the line to where the region starts. */
interface Mark {
  gen: number;
  rowsIn: number;
  col: number;
  held: boolean;
  cells: number;
}

type Item =
  | { kind: 'output'; out: RegionOutput }
  | { kind: 'local'; text: string }
  | { kind: 'pad'; text: string }
  | { kind: 'parsed'; then: () => void };

/** Writes to one xterm in order, and applies the region rules. */
export class RegionWriter {
  private readonly term: RegionTerminal;
  /** The region the latest write left open, by the order writes go in. */
  private openGen: number | null = null;
  /** The line ends the open region wrote after its mark. */
  private openBreaks = 0;
  /** The live render the open region goes back to, while it shows a
   *  preview. */
  private restore: { gen: number; text: string } | null = null;
  /** Where the last mark came, as xterm parsed it. */
  private mark: Mark | null = null;
  /** Line ends an output held back, written before the next write lands
   *  at the cursor. */
  private pendingHold = '';
  /** `pendingHold` is the line end your echo ended on, and no session
   *  output came since, so the line ends the next one holds back come
   *  after it. */
  private echoHeld = false;
  /** The row a pinned prompt left is where the next write lands. */
  private pinRow = false;
  /** Told where a replace that writes nothing erased from. */
  private erased: ((row: number, col: number) => void) | null = null;
  /** A write waits for xterm to parse what came before it. */
  private busy = false;
  /** Writes xterm has not called back for yet. */
  private held = 0;
  /** xterm is calling back for a write, and still keeps it. */
  private calling = false;
  /** xterm called back for a write and still holds others, so it keeps
   *  the writes it parsed until it has parsed them all. */
  private parsing = false;
  /** A size that waits for xterm to parse every write it holds. */
  private size: { cols: number; rows: number } | null = null;
  private readonly queue: Item[] = [];
  private readonly osc: { dispose(): void };
  private disposed = false;
  /** The bytes your echo starts with, which it leaves out after a prompt
   *  that ends in `>`. Empty while the mark is off. */
  private echoMark = DEFAULT_ECHO_MARK;

  constructor(term: RegionTerminal) {
    this.term = term;
    this.osc = term.parser.registerOscHandler(REGION_OSC, (data) => {
      this.onMark(data);
      return true;
    });
  }

  /** Write one session output. */
  output(out: RegionOutput): void {
    this.push({ kind: 'output', out });
  }

  /** Write text the webview drew itself, such as your typed echo. It
   *  lands after the open region, so it closes it. */
  local(text: string): void {
    if (text.length > 0) this.push({ kind: 'local', text });
  }

  /** Take the mark your echo starts with now, as echoMark builds it,
   *  empty for none. */
  setEchoMark(mark: string): void {
    this.echoMark = mark;
  }

  /** Line ends that bring the cursor down to the last row, as padding
   *  after a resize. While a region is open they write nothing. They
   *  would land after it and close it while the session still repaints
   *  it, and your echo would start on a row of its own below your
   *  prompt. The region keeps its place, and the next output fills the
   *  rows below it. */
  pad(text: string): void {
    if (text.length > 0) this.push({ kind: 'pad', text });
  }

  /** Call `then` with the buffer row and column each replace that writes
   *  nothing erases from, such as your prompt leaving the text for the
   *  band, so what was drawn there can go with it. */
  onErase(then: (row: number, col: number) => void): void {
    this.erased = then;
  }

  /** How many rows the held line ends take once they are written, so
   *  padding can leave room for them. */
  pendingRows(): number {
    return (this.pendingHold.match(/\n/g) ?? []).length;
  }

  /** Where the open region starts in xterm's buffer: the buffer row and
   *  column of its first cell, with its generation. Null while no region
   *  is open, once anything was written after it, and while xterm has not
   *  parsed its mark yet. The prompt card lays the open row out from here
   *  to map a pointer to a piece. */
  region(): { gen: number; row: number; col: number } | null {
    const mark = this.mark;
    if (this.openGen === null || !mark || mark.gen !== this.openGen) return null;
    const start = this.startOf(mark);
    return start ? { gen: mark.gen, ...start } : null;
  }

  /** Run `then` once xterm has parsed everything written before it, and
   *  after any resize that made xterm parse it. */
  whenParsed(then: () => void): void {
    this.push({ kind: 'parsed', then });
  }

  /** Resize xterm to `cols` by `rows`, at once unless xterm is between
   *  two slices of what it holds. Then once it has parsed it all, before
   *  anything written after. */
  resize(cols: number, rows: number): void {
    if (this.disposed) return;
    this.size = { cols, rows };
    if (this.ready()) this.resizeNow();
  }

  dispose(): void {
    this.disposed = true;
    this.osc.dispose();
    this.mark = null;
    this.queue.length = 0;
  }

  /** A mark xterm parsed: region `gen` starts at the cursor. */
  private onMark(data: string): void {
    const match = /^o;(\d+)$/.exec(data);
    if (!match) return;
    const buffer = this.term.buffer.active;
    const row = buffer.baseY + buffer.cursorY;
    const rowsIn = row - this.lineStart(row);
    const col = buffer.cursorX;
    this.mark = {
      gen: Number(match[1]),
      rowsIn,
      col,
      held: col >= this.term.cols,
      cells: rowsIn * this.term.cols + col,
    };
  }

  /** The first row of the line row `y` is in: up past each row that goes
   *  on from the row above it. */
  private lineStart(y: number): number {
    const buffer = this.term.buffer.active;
    let row = y;
    while (row > 0 && buffer.getLine(row)?.isWrapped) row--;
    return row;
  }

  /** Where the open region of `mark` starts in xterm's buffer now: the
   *  line its last line end left the cursor on, up as many lines as it
   *  wrote line ends. xterm never reflows the line the cursor is on, so a
   *  region all on that line keeps the rows it was written in. A line
   *  above it wraps at today's width. Null when its line left the buffer
   *  or scrolled above the screen. */
  private startOf(mark: Mark): { row: number; col: number } | null {
    const buffer = this.term.buffer.active;
    let line = this.lineStart(buffer.baseY + buffer.cursorY);
    for (let k = 0; k < this.openBreaks; k++) {
      if (line <= 0) return null;
      line = this.lineStart(line - 1);
    }
    let start: { row: number; col: number };
    if (this.openBreaks === 0) {
      start = mark.held
        ? { row: line + mark.rowsIn + 1, col: 0 }
        : { row: line + mark.rowsIn, col: mark.col };
    } else {
      const cols = this.term.cols;
      start = { row: line + Math.floor(mark.cells / cols), col: mark.cells % cols };
    }
    if (start.row < buffer.baseY) return null;
    return start;
  }

  private push(item: Item): void {
    if (this.disposed) return;
    if (this.busy || this.size) this.queue.push(item);
    else this.run(item);
  }

  private drain(): void {
    while (!this.busy && !this.size && this.queue.length > 0) {
      const item = this.queue.shift();
      if (item) this.run(item);
    }
  }

  /** Call `then` once xterm has parsed every write so far. xterm may
   *  call it from inside a resize, before the buffer takes the new size. */
  private wait(then: () => void): void {
    this.send(NOTHING, then);
  }

  /** xterm can take a size now and parse each write it holds once. */
  private ready(): boolean {
    return !this.parsing && !this.calling;
  }

  /** Hand `data` to xterm, and count it until xterm calls back for it.
   *  xterm may call back twice, so `then` runs only the first time. A
   *  write xterm refuses does not count. */
  private send(data: string | Uint8Array, then?: () => void): void {
    this.held += 1;
    let done = false;
    try {
      this.term.write(data, () => {
        if (done) return;
        done = true;
        this.held -= 1;
        this.calling = true;
        try {
          then?.();
        } finally {
          this.calling = false;
        }
        // xterm keeps this write until it has parsed every one left, those
        // `then` wrote included.
        this.parsing = this.held > 0;
        // xterm calls back before it lets go of the write, so a size that
        // waits goes in once its loop is done.
        if (!this.parsing && this.size) {
          queueMicrotask(() => {
            if (this.ready()) this.resizeNow();
          });
        }
      });
    } catch (err) {
      this.held -= 1;
      throw err;
    }
  }

  /** Take the size that waits, then let the writes behind it go. */
  private resizeNow(): void {
    const size = this.size;
    this.size = null;
    if (!size || this.disposed) return;
    try {
      this.term.resize(size.cols, size.rows);
    } catch {
      // xterm takes no size before its page lays it out. The next fit
      // tries again.
    }
    this.drain();
  }

  /** Run `then` after xterm parses every write so far, holding the queue
   *  until it has. It runs at once, even inside a resize, so it reads the
   *  marker and cursor at the width they were parsed at. */
  private afterParse(then: () => void): void {
    this.busy = true;
    this.wait(() => {
      this.busy = false;
      // The terminal went away while the write waited.
      if (this.disposed) return;
      then();
      this.drain();
    });
  }

  private run(item: Item): void {
    if (item.kind === 'parsed') {
      // Code outside the writer runs once xterm is done, never in the
      // middle of a resize.
      const { then } = item;
      this.wait(() => queueMicrotask(then));
      return;
    }
    if (item.kind === 'pad') {
      if (this.openGen === null) this.run({ kind: 'local', text: item.text });
      return;
    }
    // A preview goes back to the live render before anything lands after
    // it (rule d).
    if (this.restore && this.restore.gen === this.openGen && this.lands(item)) {
      this.afterParse(() => {
        this.putRestore();
        this.run(item);
      });
      return;
    }
    if (item.kind === 'local') {
      // While your prompt shows pinned, the line ends your echo ends on
      // wait as held line ends do, so a reply that brings only a prompt
      // leaves the text on your echo's row, as the last line of any
      // other reply does. Only a pinned prompt brings held line ends or
      // leaves its row open.
      const pinned = this.pendingHold.length > 0 || this.pinRow;
      this.landText(item.text, undefined, pinned);
      return;
    }
    const { replace } = item.out;
    const readsBuffer =
      (item.out.fresh === true && item.out.text.length > 0) ||
      (replace !== undefined &&
        (replace.gen === this.openGen || (replace.fresh && replace.text.length > 0)));
    if (readsBuffer) this.afterParse(() => this.apply(item.out, true));
    else this.apply(item.out, false);
  }

  /** `item` writes something after the open region. A replace of the
   *  region itself goes first and takes its place, restore and all. */
  private lands(item: Item): boolean {
    if (item.kind === 'local') return item.text.length > 0;
    if (item.kind === 'parsed' || item.kind === 'pad') return false;
    const { text, replace } = item.out;
    if (replace && replace.gen === this.openGen) return false;
    if (text.length > 0) return true;
    return replace !== undefined && replace.fresh && replace.text.length > 0;
  }

  /** Apply one output. `parsed` says xterm has parsed every earlier
   *  write, so the marker and cursor can be read. */
  private apply(out: RegionOutput, parsed: boolean): void {
    const { replace } = out;
    if (replace && parsed) {
      const back = replace.gen === this.openGen ? this.locateAt(replace.gen) : null;
      if (back !== null) {
        // The lines above the region go with it when they are there.
        const above = replace.above ? this.locateAbove(back, replace.above.plain) : null;
        const from = above ?? back;
        const text = above && replace.above ? replace.above.text : replace.text;
        this.write(from.escape + text);
        if (text.length === 0) this.erased?.(from.row, from.col);
        if (this.pendingHold.length === 0 && replace.tail) this.pendingHold = replace.tail;
      } else if (replace.fresh && replace.text.length > 0) {
        // Held line ends end their row, which xterm has not parsed yet.
        const held = this.writeHold();
        const lead = held || this.term.buffer.active.cursorX === 0 ? '' : '\r\n';
        this.write(this.land(lead + replace.text));
        this.pendingHold = replace.tail ?? '';
      }
    }
    if (out.text.length > 0) {
      // Held line ends end their row, which xterm has not parsed yet.
      const fresh = out.fresh === true && parsed && !this.writeHold();
      const lead = fresh && this.term.buffer.active.cursorX !== 0 ? '\r\n' : '';
      this.landText(lead + out.text, () => this.settle(out, true));
    } else this.settle(out, false);
  }

  /** Keep what `out` says about the line ends to hold, the pinned row and
   *  the restore, once its text is written. `wrote` says it wrote text. */
  private settle(out: RegionOutput, wrote: boolean): void {
    const hold = out.hold ?? '';
    if (wrote) {
      this.pendingHold = hold;
    } else if (this.echoHeld) {
      // The game's line ends come after your echo's own.
      if (hold.length > 0) {
        this.pendingHold += hold;
        this.echoHeld = false;
      }
    } else if (hold.length > this.pendingHold.length) {
      this.pendingHold = hold;
    }
    if (out.pinRow !== undefined) this.pinRow = out.pinRow;
    if (out.restore !== undefined && this.openGen !== null) {
      this.restore = { gen: this.openGen, text: out.restore };
    }
  }

  /** Write `text` at the cursor after the held line ends, then run
   *  `then`. Your echo waits for xterm to parse what came before it, so
   *  the row it lands on can decide its mark. `hold` keeps back the line
   *  ends it ends on, as held line ends. */
  private landText(text: string, then?: () => void, hold = false): void {
    this.writeHold();
    if (!this.marked(text)) {
      this.writeLanded(this.land(text), hold);
      then?.();
      return;
    }
    this.afterParse(() => {
      this.writeLanded(this.withoutMark(this.land(text)), hold);
      then?.();
    });
  }

  /** Write `text`, keeping back the line ends it ends on when `hold`
   *  says so. */
  private writeLanded(text: string, hold: boolean): void {
    const at = hold ? lineEndsStart(text) : text.length;
    this.write(text.slice(0, at));
    if (at < text.length) {
      this.pendingHold = text.slice(at);
      this.echoHeld = true;
    }
  }

  /** Whether `text` starts with your mark. Never with the mark off. */
  private marked(text: string): boolean {
    return this.echoMark.length > 0 && text.startsWith(this.echoMark);
  }

  /** Your echo `text` without its mark when the row it lands on already
   *  ends in `>` before the cursor. A cursor past the last column writes
   *  on the next row, which holds nothing yet. Call it only once xterm
   *  has parsed every earlier write. */
  private withoutMark(text: string): string {
    const buffer = this.term.buffer.active;
    const x = buffer.cursorX;
    if (!this.marked(text) || x >= this.term.cols) return text;
    const before = buffer.getLine(buffer.baseY + buffer.cursorY)?.translateToString(false, 0, x);
    return endsInPrompt(before ?? '') ? text.slice(this.echoMark.length) : text;
  }

  /** `text` as it lands at the cursor: without the line end that would
   *  end the row a pinned prompt left, while that row is open. */
  private land(text: string): string {
    if (!this.pinRow) return text;
    const landed = closePinRow(text);
    if (landed.closed) this.pinRow = false;
    return landed.text;
  }

  /** Write the held line ends, which close the open region, since they
   *  come after it. True when there were any. */
  private writeHold(): boolean {
    if (this.pendingHold.length === 0) return false;
    const hold = this.pendingHold;
    this.pendingHold = '';
    this.echoHeld = false;
    this.write(hold);
    return true;
  }

  /** Put the live render back over the open region. */
  private putRestore(): void {
    const restore = this.restore;
    this.restore = null;
    if (!restore || restore.gen !== this.openGen) return;
    const back = this.locate(restore.gen);
    if (back !== null) this.write(back + restore.text);
  }

  /** Write `text` after everything else. The region it leaves open is
   *  the one its last mark starts, so text with no mark closes it. */
  private write(text: string): void {
    if (text.length === 0) return;
    this.send(text);
    this.openGen = lastMark(text);
    this.openBreaks = this.openGen === null ? 0 : breaksAfterMark(text);
    if (this.restore && this.restore.gen !== this.openGen) this.restore = null;
  }

  /** The escape back to the start of open region `gen` from the cursor,
   *  erasing to the end of the screen. Empty when the region wrote
   *  nothing past its mark. Null when xterm no longer holds its start on
   *  the screen. Call it only once xterm has parsed every earlier write. */
  private locate(gen: number): string | null {
    return this.locateAt(gen)?.escape ?? null;
  }

  /** [`locate`], with the buffer row and column the region starts at. */
  private locateAt(gen: number): { escape: string; row: number; col: number } | null {
    const mark = this.mark;
    if (!mark || mark.gen !== gen || gen !== this.openGen) return null;
    const at = this.startOf(mark);
    if (!at) return null;
    const buffer = this.term.buffer.active;
    const cursor = buffer.baseY + buffer.cursorY;
    if (at.row > cursor) return { escape: '', row: at.row, col: at.col };
    return { escape: eraseBack(cursor - at.row, at.col), row: at.row, col: at.col };
  }

  /** The escape back to the first row of the lines `plain` holds, when
   *  they sit on the screen right above the region that starts at
   *  `region`, erasing to the end of the screen. Null when they are not
   *  there. */
  private locateAbove(
    region: { row: number; col: number },
    plain: string,
  ): { escape: string; row: number; col: number } | null {
    if (region.col !== 0) return null;
    const buffer = this.term.buffer.active;
    const cursor = buffer.baseY + buffer.cursorY;
    if (region.row > cursor) return null;
    const want = squeeze(plain).length;
    const rows: string[] = [];
    for (let row = region.row - 1; row >= buffer.baseY && region.row - row <= ABOVE_ROWS; row--) {
      rows.unshift(buffer.getLine(row)?.translateToString(true) ?? '');
      if (showsLines(rows, plain)) return { escape: eraseBack(cursor - row, 0), row, col: 0 };
      if (squeeze(rows.join('')).length > want) return null;
    }
    return null;
  }
}

/** Where the run of `\r` and `\n` that `text` ends on starts, when it
 *  holds a line end. `text.length` when there is none. The native grid's
 *  twin is `line_ends_start` in regions.rs. */
function lineEndsStart(text: string): number {
  let at = text.length;
  while (at > 0 && (text[at - 1] === '\r' || text[at - 1] === '\n')) at--;
  return text.slice(at).includes('\n') ? at : text.length;
}
