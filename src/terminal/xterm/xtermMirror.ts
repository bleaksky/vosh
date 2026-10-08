// Whether the live pane's xterm copy takes the session's writes.
//
// Under the macOS underlay the native grid draws the terminal, and the
// xterm copy stays mounted, hidden, only for the cell size it measures
// (terminal.css). Writing every output into it parsed and redrew a
// terminal nobody sees, a few milliseconds of page time per step. So
// while the underlay owns the screen the copy takes no writes. When the
// screen comes back to xterm, the copy starts over from the session's
// scrollback, the way a reload onto xterm fills it, and the writes that
// arrive meanwhile follow in order.

/** What the mirror asks of the pane it serves. */
export interface MirrorHost {
  /** True while the native grid draws the screen and the copy hides. */
  owned(): boolean;
  /** The copy shows again. Fill it anew, then call `done`. */
  rebuild(done: () => void): void;
}

/** The root marks the underlay once the native surface is up, which is
 *  what hides the copy. */
export function underlayShows(root: { dataset: DOMStringMap } | undefined): boolean {
  return root?.dataset.underlay === '1';
}

export class XtermMirror {
  private on: boolean;
  private rebuilding = false;
  // Counts fills, so a fill the screen overtook settles nothing.
  private fills = 0;
  private readonly waiting: (() => void)[] = [];

  constructor(private readonly host: MirrorHost) {
    this.on = !host.owned();
  }

  /** Whether the copy takes writes now. Checks the screen first. */
  mirrors(): boolean {
    this.check();
    return this.on;
  }

  /** Look at the screen again. When the copy shows again, it fills anew
   *  from the scrollback before any other write lands. */
  check(): void {
    const on = !this.host.owned();
    if (on === this.on) return;
    this.on = on;
    if (on) {
      this.start((done) => this.host.rebuild(done));
      return;
    }
    this.fills += 1;
    this.waiting.length = 0;
    this.rebuilding = false;
  }

  /** Fill the copy anew with `fill` while it shows, the way it fills when
   *  the screen comes back. Nothing while the native grid owns it, since
   *  the copy fills anew when the screen comes back anyway. */
  refill(fill: (done: () => void) => void): void {
    this.check();
    if (this.on) this.start(fill);
  }

  /** Run a fill, holding the writes that arrive meanwhile until it ends. */
  private start(fill: (done: () => void) => void): void {
    this.fills += 1;
    this.waiting.length = 0;
    const current = this.fills;
    this.rebuilding = true;
    fill(() => {
      if (current !== this.fills) return;
      this.rebuilding = false;
      for (const write of this.waiting.splice(0)) write();
    });
  }

  /** Run one write on the copy: now, after a fill in progress, or not
   *  at all while the native grid owns the screen. */
  write(write: () => void): void {
    this.check();
    if (!this.on) return;
    if (this.rebuilding) {
      this.waiting.push(write);
      return;
    }
    write();
  }
}
