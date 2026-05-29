// Word-wrap pre-processor for incoming MUD output.
//
// xterm.js wraps at the column boundary by default — long lines break
// mid-character. When the user enables the "word wrap" toggle we
// intercept the byte stream between session://output and term.write,
// track the current visual column, and inject \r\n at the last
// whitespace boundary so wraps land between words.
//
// State is kept across calls so a line that straddles chunks still
// wraps at the right place. ANSI escape sequences pass through
// without contributing to the column count.

type AnsiState = 'normal' | 'esc' | 'csi' | 'osc';

export class WordWrapper {
  private cols: number;
  private state: AnsiState = 'normal';
  /** Chars accumulated since the last whitespace (or the start of the
   *  current line). Held back from emission until either the next
   *  whitespace flushes them, a wrap inserts a break before them, or
   *  the chunk ends and we flush whatever is left. */
  private pendingWord = '';
  /** Visible character count contained in `pendingWord`. */
  private pendingWordCols = 0;
  /** Visible column position of the cursor on the current line,
   *  including both already-emitted chars and the chars sitting in
   *  pendingWord. Reset on newline. */
  private currentCol = 0;

  constructor(cols: number) {
    this.cols = Math.max(1, cols);
  }

  setCols(cols: number) {
    this.cols = Math.max(1, cols);
  }

  /** Reset all state. Use when the upstream stream is interrupted
   *  (e.g. disconnect + reconnect) so stale half-words don't bleed
   *  into the next session. */
  reset() {
    this.state = 'normal';
    this.pendingWord = '';
    this.pendingWordCols = 0;
    this.currentCol = 0;
  }

  /** Walk `input` once, splitting at word boundaries when the column
   *  count would exceed the configured width. Returns the bytes to
   *  hand to `term.write`. Unflushed chars at the end of `input`
   *  ARE included so the user sees prompts and other line-terminated
   *  trailing content. A rare consequence: a word straddling two
   *  chunks may get split mid-character if a wrap fires between
   *  chunks. */
  process(input: string): string {
    let output = '';
    for (let i = 0; i < input.length; i++) {
      const ch = input[i];
      const code = input.charCodeAt(i);

      // ANSI escape state machine. The escape characters themselves
      // pass through the pendingWord buffer but never contribute to
      // currentCol — they have zero visible width.
      if (this.state !== 'normal') {
        this.pendingWord += ch;
        if (this.state === 'esc') {
          if (ch === '[') this.state = 'csi';
          else if (ch === ']') this.state = 'osc';
          else this.state = 'normal';
        } else if (this.state === 'csi') {
          // CSI ends on any byte in 0x40-0x7E.
          if (code >= 0x40 && code <= 0x7e) this.state = 'normal';
        } else if (this.state === 'osc') {
          // OSC ends on BEL (0x07) or ST (0x9c). ESC \ termination
          // requires lookahead; we treat ESC inside OSC as resetting
          // back to esc so the next char picks it up.
          if (code === 0x07 || code === 0x9c) this.state = 'normal';
          else if (code === 0x1b) this.state = 'esc';
        }
        continue;
      }

      if (code === 0x1b) {
        this.pendingWord += ch;
        this.state = 'esc';
        continue;
      }

      // \n and \r both move the cursor to column 0 in our model.
      // Flush pendingWord first so the terminator lands at the right
      // spot, then reset.
      if (ch === '\n' || ch === '\r') {
        output += this.pendingWord + ch;
        this.pendingWord = '';
        this.pendingWordCols = 0;
        this.currentCol = 0;
        continue;
      }

      // Whitespace flushes pendingWord. Check first whether emitting
      // pendingWord + this whitespace would overflow; if so, wrap
      // BEFORE the pending word so the word stays intact on the next
      // line.
      if (ch === ' ' || ch === '\t') {
        const projected = this.currentCol + 1;
        if (projected > this.cols && this.pendingWord.length > 0) {
          // Wrap: drop a newline, then emit the word and the
          // whitespace on the new line.
          output += '\r\n' + this.pendingWord + ch;
          this.currentCol = this.pendingWordCols + 1;
        } else {
          output += this.pendingWord + ch;
          this.currentCol = projected;
        }
        this.pendingWord = '';
        this.pendingWordCols = 0;
        continue;
      }

      // Any other visible character. Accumulate into pendingWord
      // until either whitespace flushes it or we hit the column
      // boundary inside the word.
      this.pendingWord += ch;
      this.pendingWordCols += 1;
      this.currentCol += 1;

      if (this.currentCol > this.cols) {
        if (this.pendingWordCols < this.currentCol) {
          // There was already some emitted content on this line
          // before the current word. Wrap before the word: drop a
          // newline and start the new line with pendingWord.
          output += '\r\n' + this.pendingWord;
          this.currentCol = this.pendingWordCols;
          this.pendingWord = '';
          this.pendingWordCols = 0;
        } else {
          // The whole line so far IS the current word (a single
          // unbroken token longer than the terminal width). Emit it
          // and let xterm hard-wrap; reset so the next wrap point
          // can be tracked normally.
          output += this.pendingWord;
          this.pendingWord = '';
          this.pendingWordCols = 0;
          // currentCol stays — xterm has already advanced through
          // those chars at the character-wrap fallback.
        }
      }
    }

    // Flush whatever remains at the end of the chunk so prompts and
    // other unterminated content are visible.
    output += this.pendingWord;
    this.pendingWord = '';
    this.pendingWordCols = 0;
    return output;
  }
}
