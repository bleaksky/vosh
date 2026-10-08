// What one xterm makes of each session output before its RegionWriter
// writes it: the bytes decoded as UTF-8 across outputs, so a character a
// read split decodes whole, and word wrapped at the terminal's width, the
// way the native grid wraps the same stream (crates/prompt/src/wrap.rs),
// with washed rows painted in the theme's colors (src/terminal/xterm/
// xtermWash.ts) once the wrap has made every row its own.
// Terminal.tsx feeds every session://output through one, and the tests
// replay the session's payloads through the same steps.

import type { SessionOutput } from '../ipc/terminal';
import type { RegionOutput, RegionReplace } from './terminalRegion';
import { WordWrapper } from './wordWrap';
import { WashPainter, type WashFields } from './xterm/xtermWash';

export interface Shaped {
  /** What the writer writes, or null when the output writes nothing. */
  output: RegionOutput | null;
  /** The output's own text, decoded, for the recent names cache. */
  text: string;
}

export class OutputShaper {
  private readonly wrapper: WordWrapper;
  private readonly decoder = new TextDecoder('utf-8', { fatal: false });
  private readonly replaceDecoder = new TextDecoder('utf-8', { fatal: false });
  private readonly painter: WashPainter;

  /** `fields` gives the wash fields of the theme in force, and `onWash`
   *  hears each washed row painted. */
  constructor(
    cols: number,
    private readonly fields: () => WashFields = () => new Map(),
    private readonly onWash?: () => void,
  ) {
    this.wrapper = new WordWrapper(cols);
    this.painter = new WashPainter(fields, onWash);
  }

  setCols(cols: number): void {
    this.wrapper.setCols(cols);
  }

  /** Wrap a whole chunk: complete lines and the partial at its end. */
  private wrapChunk(text: string): string {
    return this.wrapper.process(text) + this.wrapper.flush();
  }

  /** Wrap and paint text that stands alone: a region's own text, or the
   *  scrollback a pane fills anew from, the way live output wraps. */
  whole(text: string): string {
    return WashPainter.whole(this.wrapChunk(text), this.fields(), this.onWash);
  }

  /** Wrap and paint the next part of the stream. */
  private streamChunk(text: string): string {
    return this.painter.paint(this.wrapChunk(text));
  }

  /** The output's text and its replace's text, decoded as `shape`
   *  decodes them but not wrapped, for a terminal that writes nothing
   *  and only hands the text to the recent names cache. */
  text(out: SessionOutput): { text: string; replace: string | null } {
    let replace: string | null = null;
    if (out.replace) {
      this.decoder.decode();
      replace = this.replaceDecoder.decode(out.replace.bytes);
    }
    const text = this.decoder.decode(out.bytes, { stream: true });
    if (out.hold) this.decoder.decode(out.hold, { stream: true });
    return { text, replace };
  }

  shape(out: SessionOutput): Shaped {
    let replace: RegionReplace | undefined;
    if (out.replace) {
      // A replace rewrites its region whole, so half a character the
      // last output held back goes with it.
      this.decoder.decode();
      this.painter.drop();
      replace = {
        gen: out.replace.gen,
        text: this.whole(this.replaceDecoder.decode(out.replace.bytes)),
        fresh: out.replace.fresh,
      };
      if (out.replace.above) {
        replace.above = {
          plain: out.replace.above.plain,
          text: this.whole(this.replaceDecoder.decode(out.replace.above.bytes)),
        };
      }
      // The end of the region the text leaves out follows it.
      if (out.replace.tail) {
        replace.tail = this.whole(this.replaceDecoder.decode(out.replace.tail));
      }
    }
    const text = this.decoder.decode(out.bytes, { stream: true });
    const wrapped = this.streamChunk(text);
    const restore = out.restore ? this.whole(this.replaceDecoder.decode(out.restore)) : undefined;
    // Held line ends follow the text in the stream, so they wrap after it.
    const hold = out.hold ? this.streamChunk(this.decoder.decode(out.hold, { stream: true })) : '';
    // Whether a pinned prompt's row is open reaches the writer even when
    // the output writes nothing else.
    if (
      wrapped.length === 0 &&
      replace === undefined &&
      restore === undefined &&
      hold === '' &&
      out.pinRow === undefined
    ) {
      return { output: null, text };
    }
    const output: RegionOutput = { text: wrapped };
    if (replace) output.replace = replace;
    if (restore !== undefined) output.restore = restore;
    if (hold.length > 0) output.hold = hold;
    if (out.pinRow !== undefined) output.pinRow = out.pinRow;
    if (out.fresh === true && wrapped.length > 0) output.fresh = true;
    return { output, text };
  }
}
