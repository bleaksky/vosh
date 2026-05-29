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

  /** Walk `line` once, tracking visible column position with ANSI
   *  escape sequences contributing zero width. When the column count
   *  exceeds the configured width, insert a CRLF at the last
   *  whitespace position so the wrap lands between words. If the
   *  current word itself is wider than the terminal, fall through to
   *  a hard wrap at the column boundary so the line still terminates. */
  private wrapLine(line: string): string {
    let output = '';
    let visibleCol = 0;
    // Position in `output` of the most recent whitespace on this line.
    // -1 means no whitespace has been seen yet (the line starts with a
    // word).
    let lastWsOutputPos = -1;
    let visibleColAtLastWs = 0;
    let state: AnsiState = 'normal';

    const handleAnsi = (ch: string, code: number) => {
      output += ch;
      if (state === 'esc') {
        if (ch === '[') state = 'csi';
        else if (ch === ']') state = 'osc';
        else state = 'normal';
        return;
      }
      if (state === 'csi') {
        if (code >= 0x40 && code <= 0x7e) state = 'normal';
        return;
      }
      if (state === 'osc') {
        if (code === 0x07 || code === 0x9c) state = 'normal';
        else if (code === 0x1b) state = 'esc';
        return;
      }
    };

    for (let i = 0; i < line.length; i++) {
      const ch = line[i];
      const code = line.charCodeAt(i);

      if (state !== 'normal') {
        handleAnsi(ch, code);
        continue;
      }

      if (code === 0x1b) {
        output += ch;
        state = 'esc';
        continue;
      }

      output += ch;
      visibleCol += 1;

      if (ch === ' ' || ch === '\t') {
        lastWsOutputPos = output.length - 1;
        visibleColAtLastWs = visibleCol;
      }

      if (visibleCol > this.cols) {
        if (lastWsOutputPos >= 0) {
          // Replace the whitespace at lastWsOutputPos with \r\n so the
          // wrap lands between words. Characters after the whitespace
          // become the start of the next line.
          const before = output.slice(0, lastWsOutputPos);
          const after = output.slice(lastWsOutputPos + 1);
          output = before + '\r\n' + after;
          visibleCol = visibleCol - visibleColAtLastWs;
          lastWsOutputPos = -1;
          visibleColAtLastWs = 0;
        } else {
          // Single token wider than the terminal. Emit a hard wrap so
          // the terminal does not paper over with character wrap. The
          // current char stays on the new line.
          output = output.slice(0, output.length - 1) + '\r\n' + ch;
          visibleCol = 1;
        }
      }
    }
    return output;
  }
}
