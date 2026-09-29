// The command input at a password prompt. A server takes over echo with
// IAC WILL ECHO to ask for a password, and the input row turns into a
// masked field until IAC WONT ECHO hands echo back. A line submitted
// from the masked field leaves no trace in Vosh.

/** Wrap an echoed input line in a truecolor SGR sequence so you can spot
 *  your own commands. Returns the line unchanged when no color is set or
 *  the hex cannot be parsed. The reset closes before the trailing CRLF. */
export function colorizeEcho(line: string, color: string | null): string {
  if (!color) return line;
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})/i.exec(color.trim());
  if (!m) return line;
  const r = parseInt(m[1], 16);
  const g = parseInt(m[2], 16);
  const b = parseInt(m[3], 16);
  return `\x1b[38;2;${r};${g};${b}m${line}\x1b[0m`;
}

/** What the command input does with one submitted line. */
export interface SubmitPlan {
  /** The local echo for the terminal, the split history pane, and the
   *  native renderer, or null for none. */
  echo: string | null;
  /** Whether the line joins command history for Up arrow recall. */
  remember: boolean;
  /** Whether the line is a command the frontend runs itself, like
   *  `#nativesurface`, instead of sending it. */
  local: boolean;
  /** Whether the line goes out through the masked send, exactly as
   *  typed, instead of through the input pipeline. */
  masked: boolean;
}

export interface SubmitContext {
  /** The input row is the masked password field. */
  masked: boolean;
  /** The line starts with a quick key, whose expansion the backend
   *  echoes itself. */
  quickKey: boolean;
  /** The echo color from Settings, or null for the terminal default. */
  echoColor: string | null;
}

/** Plan a submitted line. A line from the masked field echoes only a line
 *  break, stays out of history, and goes to the server as typed, past
 *  aliases, variables, and `#` commands. Every other line echoes in your
 *  echo color, unless a quick key echoes it, and joins history. */
export function planSubmit(line: string, context: SubmitContext): SubmitPlan {
  if (context.masked) {
    return { echo: '\r\n', remember: false, local: false, masked: true };
  }
  return {
    echo: context.quickKey ? null : `${colorizeEcho(line, context.echoColor)}\r\n`,
    remember: line.length > 0,
    local: /^#nativesurface\b/i.test(line),
    masked: false,
  };
}
