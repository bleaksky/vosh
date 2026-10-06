// Line-buffered word wrap for incoming MUD output.
//
// Buffers chars until a newline arrives. Then word-wraps the complete
// line at the configured column width and emits it. Lines ending in
// \n (the vast majority of MUD output) are processed instantly because
// the buffer is whole when wrap runs.
//
// Anything still in the buffer at the end of process() is a partial
// line — almost always a prompt without trailing whitespace. The
// caller can flush() it after a short idle to surface that prompt
// without forcing mid-word splits inside lines that happen to span
// two TCP packets.
//
// Trade-off: a single line that arrives split across packets sees no
// wrap until the closing \n shows up. The visible effect is that the
// last few characters of the partial line do not appear until the
// flush timer fires or the next chunk lands; for live MUD output
// that means around 20 ms on prompts, zero ms on every \n-terminated
// chunk. No more "city" splitting into "ci" / "ty".
//
// The native grid wraps with the Rust twin in crates/prompt/src/wrap.rs.
// Both run fixtures/wrap/cases.json, so keep them in step.

type AnsiState = 'normal' | 'esc' | 'csi' | 'osc';

export class WordWrapper {
  private cols: number;
  private buffer = '';

  constructor(cols: number) {
    this.cols = Math.max(1, cols);
  }

  setCols(cols: number) {
    this.cols = Math.max(1, cols);
  }

  reset() {
    this.buffer = '';
  }

  /** Walk the input chunk. Emit a wrapped form of every complete line
   *  (terminator: \n or \r). Buffer the trailing partial line so a
   *  word that straddles two TCP packets still wraps in the right
   *  place when the closing \n finally arrives. */
  process(input: string): string {
    let output = '';
    for (let i = 0; i < input.length; i++) {
      const ch = input[i];
      if (ch === '\n' || ch === '\r') {
        // Complete the line, wrap it, emit with the terminator.
        output += this.wrapLine(this.buffer) + ch;
        this.buffer = '';
        continue;
      }
      this.buffer += ch;
    }
    return output;
  }

  /** Emit whatever partial line is held in the buffer. Use after a
   *  short idle so prompts that end without a newline still show up.
   *  The partial gets word-wrapped just like a complete line; a short
   *  prompt (under cols) passes through unchanged. */
  flush(): string {
    if (this.buffer.length === 0) return '';
    const out = this.wrapLine(this.buffer);
    this.buffer = '';
    return out;
  }

  /** Wrap one line at the configured width. Each break from
   *  wrapBreaks becomes a CRLF, in place of the whitespace it takes or
   *  before the character it moves down. */
  private wrapLine(line: string): string {
    const breaks = wrapBreaks(line, this.cols);
    if (breaks.length === 0) return line;
    let output = '';
    let from = 0;
    for (const { at, replaced } of breaks) {
      output += line.slice(from, at) + '\r\n';
      from = replaced ? at + 1 : at;
    }
    return output + line.slice(from);
  }
}

/** A place `wrapBreaks` breaks a line: before the character at index
 *  `at`, or in place of it when `replaced`, which it is for the
 *  whitespace a break between words takes. */
export interface WrapBreak {
  at: number;
  replaced: boolean;
}

/** Where WordWrapper breaks `line`, one line with no line ends, at
 *  `cols` wide. Walk it once, tracking the visible column with escape
 *  sequences at zero width. When the column passes the width, break at
 *  the last whitespace so the wrap lands between words. A word wider
 *  than the width breaks at the edge, before the character that went
 *  past it, so the line still ends. Breaks come in the order of the
 *  line. The prompt card lays a drawn prompt out with them to find the
 *  cell each piece landed in. */
export function wrapBreaks(line: string, cols: number): WrapBreak[] {
  const width = Math.max(1, cols);
  const breaks: WrapBreak[] = [];
  let visibleCol = 0;
  // Index in `line` of the most recent whitespace since the last break,
  // with its visible column. -1 when the segment starts with a word.
  let lastWs = -1;
  let visibleColAtLastWs = 0;
  let state: AnsiState = 'normal';

  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    const code = line.charCodeAt(i);

    if (state !== 'normal') {
      if (state === 'esc') {
        state = ch === '[' ? 'csi' : ch === ']' ? 'osc' : 'normal';
      } else if (state === 'csi') {
        if (code >= 0x40 && code <= 0x7e) state = 'normal';
      } else if (code === 0x07 || code === 0x9c) {
        state = 'normal';
      } else if (code === 0x1b) {
        state = 'esc';
      }
      continue;
    }

    if (code === 0x1b) {
      state = 'esc';
      continue;
    }

    visibleCol += 1;

    if (ch === ' ' || ch === '\t') {
      lastWs = i;
      visibleColAtLastWs = visibleCol;
    }

    if (visibleCol > width) {
      if (lastWs >= 0) {
        // The whitespace becomes the break, and what follows it starts
        // the next line.
        breaks.push({ at: lastWs, replaced: true });
        visibleCol -= visibleColAtLastWs;
        lastWs = -1;
        visibleColAtLastWs = 0;
      } else {
        // A single token wider than the terminal. The current character
        // starts the next line.
        breaks.push({ at: i, replaced: false });
        visibleCol = 1;
      }
    }
  }
  return breaks;
}
