// The command input at a password prompt. A server takes over echo with
// IAC WILL ECHO to ask for a password, and the input row turns into a
// masked field until IAC WONT ECHO hands echo back. A line submitted
// from the masked field leaves no trace in Vosh.

import { DEFAULT_ECHO_MARK_OPTIONS, type EchoMarkOptions } from '../ipc/uiConfigInput';

/** The `r;g;b` of a Command or Mark color, the six hex digits at its
 *  start after an optional `#`, or null when it does not read. */
function echoRgb(color: string | null): string | null {
  const m = color ? /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})/i.exec(color.trim()) : null;
  if (!m) return null;
  return m
    .slice(1)
    .map((hex) => parseInt(hex, 16))
    .join(';');
}

/** The command of an echo in the Command color when one reads, faint
 *  when `dim` is on, or as it is with neither. The reset closes before
 *  the line end. */
function styleCommand(line: string, color: string | null, dim: boolean): string {
  const rgb = echoRgb(color);
  const sgr = [dim ? '2' : '', rgb ? `38;2;${rgb}` : ''].filter(Boolean).join(';');
  return sgr ? `\x1b[${sgr}m${line}\x1b[0m` : line;
}

/** The text of the mark you picked, `›`, `>` or your own, empty while
 *  it is off. The echo and the command line both draw it. */
export function markText({ mark, text }: Pick<EchoMarkOptions, 'mark' | 'text'>): string {
  return mark === 'off' ? '' : mark === 'gt' ? '>' : mark === 'own' ? text : '\u203a';
}

/** The mark before each command you send, empty while it is off or your
 *  own text is blank: the Mark color, or the theme's bright black (SGR
 *  90) when none reads, so both renderers draw it in the active theme,
 *  then `›`, `>` or your own text, then a space and a reset. U+203A is
 *  one cell wide. The backend builds a quick key's mark the same way
 *  (echo_mark in src-tauri/src/input.rs), and
 *  fixtures/input/echo-marks.json holds both to the same bytes. Each
 *  renderer leaves it out when the row your echo lands on already ends
 *  in `>`, as a game's prompt such as `Account name> ` does (RegionWriter
 *  in terminalRegion.ts, and the native grid). */
export function echoMark({
  mark,
  text,
  color,
}: Pick<EchoMarkOptions, 'mark' | 'text' | 'color'>): string {
  const glyph = markText({ mark, text });
  if (glyph.length === 0) return '';
  const rgb = echoRgb(color);
  return `\x1b[${rgb ? `38;2;${rgb}` : '90'}m${glyph} \x1b[0m`;
}

/** The mark of a profile that never changed it, the grey `›`. */
export const DEFAULT_ECHO_MARK = echoMark(DEFAULT_ECHO_MARK_OPTIONS);

/** The echo of one command, with its line end: the `mark` from
 *  echoMark, then the command in the Command color, faint when `dim` is
 *  on. A bare Enter echoes the mark alone, so you see each blank line
 *  you send. */
export function commandEcho(
  line: string,
  color: string | null,
  mark: string,
  dim: boolean,
): string {
  return `${mark}${styleCommand(line, color, dim)}\r\n`;
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
  /** The Command color from Settings, or null for the terminal default. */
  echoColor: string | null;
  /** The mark from Mark before your commands, built by echoMark, empty for
   *  none. */
  echoMark: string;
  /** Dim sent commands from Settings. */
  echoDim: boolean;
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
  return commandEcho(command, context.echoColor, context.echoMark, context.echoDim);
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
 *  Command color after your mark, unless a quick key echoes it, and
 *  joins history. A bare Enter echoes too, the mark alone or an empty
 *  line with the mark off, as a telnet client shows each line you
 *  send, pinned prompt or not. */
export function planSubmit(line: string, context: SubmitContext): SubmitPlan {
  if (context.masked) {
    return { echo: '\r\n', remember: false, local: false, masked: true };
  }
  const silent = context.quickKey;
  return {
    echo: silent ? null : commandEcho(line, context.echoColor, context.echoMark, context.echoDim),
    remember: line.length > 0,
    local: /^#nativesurface\b/i.test(line),
    masked: false,
  };
}
