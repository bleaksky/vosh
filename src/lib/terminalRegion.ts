// Marked regions in xterm (the prompt build spec, D22 and section 4).
//
// The session marks where a region it may replace later starts, with the
// private mark ESC ] 7717 ; o ; G BEL. A drawn prompt, a partial line
// painted at the end of a read, and a repaint each start one. A later
// output replaces a region: xterm finds it in its own buffer, checks that
// nothing was written after it, and only then erases from its start and
// writes the new text. The session never counts rows, since your typed
// echo reaches xterm before the session hears of it, and xterm wraps at
// its own width. The native grid follows the same rules in term_grid.rs.
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

/** The private OSC a region mark uses. */
export const REGION_OSC = 7717;

/** The parts of an xterm marker the writer reads. */
export interface RegionMarker {
  readonly line: number;
  readonly isDisposed: boolean;
  dispose(): void;
}

/** The parts of xterm the writer uses. */
export interface RegionTerminal {
  readonly cols: number;
  readonly buffer: {
    readonly active: {
      readonly cursorX: number;
      readonly cursorY: number;
      readonly baseY: number;
      getLine(y: number): { translateToString(trimRight?: boolean): string } | undefined;
    };
  };
  readonly parser: {
    registerOscHandler(
      ident: number,
      callback: (data: string) => boolean | Promise<boolean>,
    ): { dispose(): void };
  };
  write(data: string | Uint8Array, callback?: () => void): void;
  registerMarker(cursorYOffset?: number): RegionMarker | undefined;
}

/** Replace region `gen` with `text`. When the region is closed, a
 *  `fresh` replace writes `text` on a new row and any other is dropped.
 *  `above` carries the lines the region's prompt shows right above it,
 *  such as a tank line shown as sent: when the rows right above the open
 *  region show `plain`, the writer erases from their first row and writes
 *  its `text` there instead. */
export interface RegionReplace {
  gen: number;
  text: string;
  fresh: boolean;
  above?: { plain: string; text: string };
}

/** True when rows reading `rows`, top first, show the lines `plain`
 *  holds, however xterm or the word wrap broke them. Blanks do not count,
 *  since a word wrap drops the one it breaks at. The same rule as
 *  shows_lines in crates/prompt/src/stage.rs. */
export function showsLines(rows: string[], plain: string): boolean {
  const want = squeeze(plain);
  return want.length > 0 && squeeze(rows.join('')) === want;
}

function squeeze(text: string): string {
  return text.replace(/\s+/g, '');
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
}

/** What `text` does to the row a pinned prompt left open. The prompt is
 *  not in the text, so the line end that would end its row writes
 *  nothing: the first one, when only escape sequences and carriage
 *  returns come before it. `closed` says the row is closed, by that line
 *  end or by anything else that lands first. Escape sequences alone leave
 *  it open. The same rule as close_pin_row in crates/prompt/src/stage.rs. */
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

/** The escape that goes from the cursor at the end of a region back to
 *  its start, `above` rows up at column `col`, then erases to the end of
 *  the screen (rule c). */
export function eraseBack(above: number, col: number): string {
  return `\r${above > 0 ? `\x1b[${above}A` : ''}${col > 0 ? `\x1b[${col}C` : ''}\x1b[0J`;
}

/** Where the last mark xterm parsed came. `held` says the cursor sat past
 *  the last column, so the region starts at the next row. */
interface Mark {
  gen: number;
  marker: RegionMarker;
  col: number;
  held: boolean;
}

type Item =
  | { kind: 'output'; out: RegionOutput }
  | { kind: 'local'; text: string }
  | { kind: 'parsed'; then: () => void };

/** Writes to one xterm in order, and applies the region rules. */
export class RegionWriter {
  private readonly term: RegionTerminal;
  /** The region the latest write left open, by the order writes go in. */
  private openGen: number | null = null;
  /** The live render the open region goes back to, while it shows a
   *  preview. */
  private restore: { gen: number; text: string } | null = null;
  /** Where the last mark came, as xterm parsed it. */
  private mark: Mark | null = null;
  /** Line ends an output held back, written before the next write lands
   *  at the cursor. */
  private pendingHold = '';
  /** The row a pinned prompt left is where the next write lands. */
  private pinRow = false;
  /** Told where a replace that writes nothing erased from. */
  private erased: ((row: number, col: number) => void) | null = null;
  /** A write waits for xterm to parse what came before it. */
  private busy = false;
  private readonly queue: Item[] = [];
  private readonly osc: { dispose(): void };
  private disposed = false;

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
    if (mark.marker.isDisposed || mark.marker.line < 0) return null;
    return {
      gen: mark.gen,
      row: mark.held ? mark.marker.line + 1 : mark.marker.line,
      col: mark.held ? 0 : mark.col,
    };
  }

  /** Run `then` once xterm has parsed everything written before it, and
   *  after any resize that made xterm parse it. */
  whenParsed(then: () => void): void {
    this.push({ kind: 'parsed', then });
  }

  dispose(): void {
    this.disposed = true;
    this.osc.dispose();
    this.mark?.marker.dispose();
    this.mark = null;
    this.queue.length = 0;
  }

  /** A mark xterm parsed: region `gen` starts at the cursor. */
  private onMark(data: string): void {
    const match = /^o;(\d+)$/.exec(data);
    if (!match) return;
    this.mark?.marker.dispose();
    const marker = this.term.registerMarker(0);
    const col = this.term.buffer.active.cursorX;
    this.mark = marker ? { gen: Number(match[1]), marker, col, held: col >= this.term.cols } : null;
  }

  private push(item: Item): void {
    if (this.disposed) return;
    if (this.busy) this.queue.push(item);
    else this.run(item);
  }

  private drain(): void {
    while (!this.busy && this.queue.length > 0) {
      const item = this.queue.shift();
      if (item) this.run(item);
    }
  }

  /** Call `then` once xterm has parsed every write so far. xterm may
   *  call it from inside a resize, before the buffer takes the new size,
   *  and may call it twice, so it runs only the first time. */
  private wait(then: () => void): void {
    let done = false;
    this.term.write(NOTHING, () => {
      if (done) return;
      done = true;
      then();
    });
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
      this.writeHold();
      this.write(this.land(item.text));
      return;
    }
    const { replace } = item.out;
    const readsBuffer =
      replace !== undefined &&
      (replace.gen === this.openGen || (replace.fresh && replace.text.length > 0));
    if (readsBuffer) this.afterParse(() => this.apply(item.out, true));
    else this.apply(item.out, false);
  }

  /** `item` writes something after the open region. A replace of the
   *  region itself goes first and takes its place, restore and all. */
  private lands(item: Item): boolean {
    if (item.kind === 'local') return item.text.length > 0;
    if (item.kind === 'parsed') return false;
    const { text, replace } = item.out;
    if (replace && replace.gen === this.openGen) return false;
    if (text.length > 0) return true;
    return replace !== undefined && replace.fresh && replace.text.length > 0;
  }

  /** Apply one output. `parsed` says xterm has parsed every earlier
   *  write, so the marker and cursor can be read. */
  private apply(out: RegionOutput, parsed: boolean): void {
    const { replace } = out;
    let wrote = false;
    if (replace && parsed) {
      const back = replace.gen === this.openGen ? this.locateAt(replace.gen) : null;
      if (back !== null) {
        // The lines above the region go with it when they are there.
        const above = replace.above ? this.locateAbove(back, replace.above.plain) : null;
        const from = above ?? back;
        const text = above && replace.above ? replace.above.text : replace.text;
        this.write(from.escape + text);
        if (text.length === 0) this.erased?.(from.row, from.col);
      } else if (replace.fresh && replace.text.length > 0) {
        // Held line ends end their row, which xterm has not parsed yet.
        const held = this.writeHold();
        const lead = held || this.term.buffer.active.cursorX === 0 ? '' : '\r\n';
        this.write(this.land(lead + replace.text));
        wrote = true;
      }
    }
    if (out.text.length > 0) {
      this.writeHold();
      this.write(this.land(out.text));
      wrote = true;
    }
    const hold = out.hold ?? '';
    if (wrote || hold.length > this.pendingHold.length) this.pendingHold = hold;
    if (out.pinRow !== undefined) this.pinRow = out.pinRow;
    if (out.restore !== undefined && this.openGen !== null) {
      this.restore = { gen: this.openGen, text: out.restore };
    }
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
    this.term.write(text);
    this.openGen = lastMark(text);
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
    if (!mark || mark.gen !== gen || mark.marker.isDisposed || mark.marker.line < 0) {
      return null;
    }
    const buffer = this.term.buffer.active;
    const cursor = buffer.baseY + buffer.cursorY;
    const start = mark.held ? mark.marker.line + 1 : mark.marker.line;
    const col = mark.held ? 0 : mark.col;
    if (start > cursor) return { escape: '', row: start, col };
    if (start < buffer.baseY) return null;
    return { escape: eraseBack(cursor - start, col), row: start, col };
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
