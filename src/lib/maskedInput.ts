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
  /** The row your pinned prompt held is where the next thing lands, so
   *  the text holds no prompt row for an empty line's echo to end. False
   *  at a prompt left in the text, such as the pager. */
  pinRowOpen?: boolean;
}

/** Whether a key press or a submitted line belongs to the masked field.
 *  `rendered` is the mask the input row last rendered with, the one that
 *  goes with the draft a key handler holds. `latest` is the mask from the
 *  newest input-mode event, which can arrive a render before the row
 *  catches up. Either one masks. Right after the server takes echo, a
 *  line typed in the plain row is already a password. Right after it
 *  hands echo back, or the link drops, the draft is still the password
 *  typed in the masked field. */
export function isMasked(rendered: boolean, latest: boolean): boolean {
  return rendered || latest;
}

/** Whether Keep last command leaves the line you just sent selected in
 *  the input. Never for a line from the masked field. */
export function keepsLastCommand(enabled: boolean, composed: string, masked: boolean): boolean {
  return enabled && composed.length > 0 && !masked;
}

/** The local echo for a macro's command, or null for none. `enabled` is
 *  the Echo macros setting. A macro pressed at the masked field echoes
 *  nothing, and a quick key leaves its echo to the backend. */
export function macroEcho(
  command: string,
  context: SubmitContext & { enabled: boolean },
): string | null {
  if (!context.enabled || context.masked || context.quickKey) return null;
  return `${colorizeEcho(command, context.echoColor)}\r\n`;
}

/** The draft the input row keeps when it masks or unmasks. A flip either
 *  way empties it. Unmasking must not show a half typed password in the
 *  plain row, where Enter would echo it and put it in history. Masking
 *  must not put a leftover command, like the account name Keep last
 *  command kept, in front of the password. */
export function draftAfterMaskChange(wasMasked: boolean, masked: boolean, draft: string): string {
  return wasMasked === masked ? draft : '';
}

/** Plan a submitted line. A line from the masked field echoes only a line
 *  break, stays out of history, and goes to the server as typed, past
 *  aliases, variables, and `#` commands. Every other line echoes in your
 *  echo color, unless a quick key echoes it, and joins history. At your
 *  pinned prompt an empty line echoes nothing, so Enter on an empty line
 *  moves nothing in the text. */
export function planSubmit(line: string, context: SubmitContext): SubmitPlan {
  if (context.masked) {
    return { echo: '\r\n', remember: false, local: false, masked: true };
  }
  const silent = context.quickKey || (context.pinRowOpen === true && line.length === 0);
  return {
    echo: silent ? null : `${colorizeEcho(line, context.echoColor)}\r\n`,
    remember: line.length > 0,
    local: /^#nativesurface\b/i.test(line),
    masked: false,
  };
}
