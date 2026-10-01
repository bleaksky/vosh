// What one xterm makes of each session output before its RegionWriter
// writes it: the bytes decoded as UTF-8 across outputs, so a character a
// read split decodes whole, and word wrapped at the terminal's width, the
// way the native grid wraps the same stream (crates/prompt/src/wrap.rs).
// Terminal.tsx feeds every session://output through one, and the tests
// replay the session's payloads through the same steps.

import type { SessionOutput } from './session';
import type { RegionOutput, RegionReplace } from './terminalRegion';
import { WordWrapper } from './wordWrap';

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

  constructor(cols: number) {
    this.wrapper = new WordWrapper(cols);
  }

  setCols(cols: number): void {
    this.wrapper.setCols(cols);
  }

  /** Wrap a whole chunk: complete lines and the partial at its end. */
  private wrapChunk(text: string): string {
    return this.wrapper.process(text) + this.wrapper.flush();
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
      replace = {
        gen: out.replace.gen,
        text: this.wrapChunk(this.replaceDecoder.decode(out.replace.bytes)),
        fresh: out.replace.fresh,
      };
      if (out.replace.above) {
        replace.above = {
          plain: out.replace.above.plain,
          text: this.wrapChunk(this.replaceDecoder.decode(out.replace.above.bytes)),
        };
      }
    }
    const text = this.decoder.decode(out.bytes, { stream: true });
    const wrapped = this.wrapChunk(text);
    const restore = out.restore
      ? this.wrapChunk(this.replaceDecoder.decode(out.restore))
      : undefined;
    // Held line ends follow the text in the stream, so they wrap after it.
    const hold = out.hold ? this.wrapChunk(this.decoder.decode(out.hold, { stream: true })) : '';
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
    return { output, text };
  }
}
