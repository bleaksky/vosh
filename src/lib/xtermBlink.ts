import type { IBufferCell, IDisposable, Terminal } from '@xterm/xterm';
import { BLINK_MS, untilBlinkShows } from './blink';

// Blinking text in xterm. xterm 6.1 draws SGR 5 itself once
// blinkIntervalDuration is above 0. Its WebGL renderer hides a blinking
// cell's glyph and lines in the hidden half and keeps its ground and the
// selection. Its DOM renderer does not. The class that hides a blinking
// span leaks onto the steady span after it on the row, and it hides the
// span's ground too. So only a pane WebGL draws blinks (setWebgl), and a
// DOM pane, such as the split history pane, draws blinking text steady.
// xterm has no SGR 6, so the rapid blink draws steady everywhere.
//
// xterm flips on an interval of its own that starts when blinking text
// first shows. So while any shows, this starts that interval again on the
// shared clock (src/lib/blink.ts), at the start of a shown half, so the
// live pane, the pinned band and the native grid flip together. While
// nothing on screen blinks, it runs no timer, and once an SGR has ended
// the blink as well, it reads no render until the next SGR 5.

/** How often blinking text goes back on the shared clock while it shows,
 *  in whole cycles. xterm's interval keeps time well, so this only
 *  catches a restart of its own, as when the pane shows again. */
const REALIGN_CYCLES = 10;

/** What an SGR does to blink, from its parameters as xterm hands them to
 *  a handler: true when it leaves blink on, false when it ends it with a
 *  0, a 25 or a bare reset, and null when it leaves blink alone. The last
 *  of them counts. A 38, 48 or 58 without colons takes the numbers of its
 *  color after it, as xterm reads them, so a 5 or a 0 among them is part
 *  of the color, and a color with colons comes as one number and a list. */
export function sgrBlink(params: readonly (number | number[])[]): boolean | null {
  if (params.length === 0) return false;
  let blink: boolean | null = null;
  for (let i = 0; i < params.length; i++) {
    const p = params[i];
    if (p === 5) blink = true;
    else if (p === 0 || p === 25) blink = false;
    else if ((p === 38 || p === 48 || p === 58) && typeof params[i + 1] === 'number') {
      const mode = params[i + 1];
      i += mode === 2 ? 4 : mode === 5 ? 2 : 1;
    }
  }
  return blink;
}

/** The parts of xterm this reads. */
type BlinkTerm = Pick<Terminal, 'rows' | 'options' | 'buffer' | 'parser' | 'onRender' | 'onResize'>;

export class XtermBlink {
  /** The Blinking text setting. */
  private setting = false;
  /** WebGL draws the pane. */
  private webgl = false;
  /** xterm blinks: the setting is on and WebGL draws the pane. */
  private on = false;
  /** Blink may show: an SGR 5 reached this terminal, and no SGR has
   *  ended it since while nothing on screen blinked. Until then, and once
   *  blink has gone again, no render is read, so a game that never
   *  blinks, or blinked once long ago, costs nothing. */
  private seen = false;
  /** The last SGR the hook read ended blink, so none can be written until
   *  another 5. */
  private ended = false;
  /** Reads each SGR while no blinking text is on screen, for a 5 and for
   *  the SGR that ends blink after it. It goes at a 5, so an SGR costs
   *  nothing more while renders are read. */
  private hook: IDisposable | null = null;
  private disposed = false;
  /** Each screen row holds a blinking cell. */
  private rows: boolean[] = [];
  private blinking = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private cell: IBufferCell | undefined;
  private readonly subs: IDisposable[] = [];

  constructor(private readonly term: BlinkTerm) {
    this.hookSgr();
    this.subs.push(
      term.onRender(({ start, end }) => this.scan(start, end)),
      term.onResize(() => this.scan(0, this.term.rows - 1)),
    );
  }

  /** Blinking text on or off, from the Blinking text setting. */
  setOn(on: boolean): void {
    this.setting = on;
    this.apply();
  }

  /** WebGL draws the pane, or it went back to the DOM renderer. */
  setWebgl(live: boolean): void {
    this.webgl = live;
    this.apply();
  }

  dispose(): void {
    this.stop();
    // Off for good, so a late call leaves the terminal alone.
    this.disposed = true;
    this.setting = false;
    this.on = false;
    this.hook?.dispose();
    this.hook = null;
    for (const sub of this.subs) sub.dispose();
    this.subs.length = 0;
  }

  private apply(): void {
    const on = this.setting && this.webgl;
    if (on === this.on) return;
    this.on = on;
    this.term.options.blinkIntervalDuration = on ? BLINK_MS : 0;
    if (on) {
      this.scan(0, this.term.rows - 1);
    } else {
      this.rows = [];
      this.blinking = 0;
      this.stop();
    }
  }

  /** Hear each SGR. Whether blink is on now is not known, so it counts
   *  as on until an SGR ends it. */
  private hookSgr(): void {
    if (this.hook !== null || this.disposed) return;
    this.ended = false;
    this.hook = this.term.parser.registerCsiHandler({ final: 'm' }, (params) => {
      const blink = sgrBlink(params);
      if (blink === true) this.see();
      else if (blink === false) this.ended = true;
      // xterm applies the SGR itself.
      return false;
    });
  }

  /** An SGR 5 came, so renders are read. xterm may still be inside the
   *  hook's call, and it lets a handler go mid call. */
  private see(): void {
    this.seen = true;
    this.hook?.dispose();
    this.hook = null;
  }

  /** Read screen rows `start` to `end` for blinking cells, as xterm drew
   *  them. */
  private scan(start: number, end: number): void {
    if (!this.on || !this.seen) return;
    const buffer = this.term.buffer.active;
    const rows = this.term.rows;
    if (this.rows.length !== rows) {
      this.rows = new Array<boolean>(rows).fill(false);
      this.blinking = 0;
      start = 0;
      end = rows - 1;
    }
    for (let y = Math.max(0, start); y <= Math.min(end, rows - 1); y++) {
      const line = buffer.getLine(buffer.viewportY + y);
      let blinks = false;
      for (let x = 0; line && x < line.length && !blinks; x++) {
        this.cell = line.getCell(x, this.cell);
        blinks = (this.cell?.isBlink() ?? 0) !== 0;
      }
      if (blinks !== this.rows[y]) {
        this.rows[y] = blinks;
        this.blinking += blinks ? 1 : -1;
      }
    }
    if (this.blinking > 0) {
      if (this.timer === null) this.schedule(untilBlinkShows(Date.now()));
      return;
    }
    this.stop();
    // Nothing on screen blinks. The hook hears the SGRs again, and once
    // one has ended blink, none can be written until another 5, so
    // renders go unread again.
    if (this.hook === null) this.hookSgr();
    else if (this.ended) this.seen = false;
  }

  private schedule(wait: number): void {
    this.timer = setTimeout(() => {
      this.timer = null;
      if (!this.on || this.blinking === 0) return;
      // A new interval starts shown, here at the start of a shown half.
      this.term.options.blinkIntervalDuration = 0;
      this.term.options.blinkIntervalDuration = BLINK_MS;
      this.schedule(REALIGN_CYCLES * 2 * BLINK_MS);
    }, wait);
  }

  private stop(): void {
    if (this.timer === null) return;
    clearTimeout(this.timer);
    this.timer = null;
  }
}
