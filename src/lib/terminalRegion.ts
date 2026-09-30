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
    };
  };
  readonly parser: {
    registerOscHandler(
      ident: number,
      callback: (data: string) => boolean | Promise<boolean>,
    ): { dispose(): void };
  };
  write(data: string, callback?: () => void): void;
  registerMarker(cursorYOffset?: number): RegionMarker | undefined;
}

/** Replace region `gen` with `text`. When the region is closed, a
 *  `fresh` replace writes `text` on a new row and any other is dropped. */
export interface RegionReplace {
  gen: number;
  text: string;
  fresh: boolean;
}

/** One session output, decoded and wrapped: the replace goes first, then
 *  the text. `restore` is the live render for the region the output
 *  leaves open, which goes back over it before anything else lands. */
export interface RegionOutput {
  text: string;
  replace?: RegionReplace;
  restore?: string;
}

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
  /** A write waits for xterm to parse what came before it. */
  private busy = false;
  private readonly queue: Item[] = [];
  private readonly osc: { dispose(): void };

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

  /** Run `then` once xterm has parsed everything written before it. */
  whenParsed(then: () => void): void {
    this.push({ kind: 'parsed', then });
  }

  dispose(): void {
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
    if (this.busy) this.queue.push(item);
    else this.run(item);
  }

  private drain(): void {
    while (!this.busy && this.queue.length > 0) {
      const item = this.queue.shift();
      if (item) this.run(item);
    }
  }

  /** Run `then` after xterm parses every write so far, holding the queue
   *  until it has. */
  private afterParse(then: () => void): void {
    this.busy = true;
    this.term.write('', () => {
      this.busy = false;
      then();
      this.drain();
    });
  }

  private run(item: Item): void {
    if (item.kind === 'parsed') {
      this.term.write('', item.then);
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
      this.write(item.text);
      return;
    }
    const { replace } = item.out;
    const readsBuffer =
      replace !== undefined &&
      (replace.gen === this.openGen || (replace.fresh && replace.text.length > 0));
    if (readsBuffer) this.afterParse(() => this.apply(item.out, true));
    else this.apply(item.out, false);
  }

  /** `item` writes something after the open region. */
  private lands(item: Item): boolean {
    if (item.kind === 'local') return item.text.length > 0;
    if (item.kind === 'parsed') return false;
    const { text, replace } = item.out;
    if (text.length > 0) return true;
    return (
      replace !== undefined &&
      replace.gen !== this.openGen &&
      replace.fresh &&
      replace.text.length > 0
    );
  }

  /** Apply one output. `parsed` says xterm has parsed every earlier
   *  write, so the marker and cursor can be read. */
  private apply(out: RegionOutput, parsed: boolean): void {
    const { replace } = out;
    if (replace && parsed) {
      const back = replace.gen === this.openGen ? this.locate(replace.gen) : null;
      if (back !== null) {
        this.write(back + replace.text);
      } else if (replace.fresh && replace.text.length > 0) {
        const lead = this.term.buffer.active.cursorX === 0 ? '' : '\r\n';
        this.write(lead + replace.text);
      }
    }
    if (out.text.length > 0) this.write(out.text);
    if (out.restore !== undefined && this.openGen !== null) {
      this.restore = { gen: this.openGen, text: out.restore };
    }
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
    const mark = this.mark;
    if (!mark || mark.gen !== gen || mark.marker.isDisposed || mark.marker.line < 0) {
      return null;
    }
    const buffer = this.term.buffer.active;
    const cursor = buffer.baseY + buffer.cursorY;
    const start = mark.held ? mark.marker.line + 1 : mark.marker.line;
    if (start > cursor) return '';
    if (start < buffer.baseY) return null;
    return eraseBack(cursor - start, mark.held ? 0 : mark.col);
  }
}
