import { ANSI_SLOTS, CANONICAL_ANSI_16 } from '../../theme/baseAnsi';
import { indexedRgb } from '../../theme/color';
import type { LogScope } from '../../ipc/logs';
import type { XtermPalette } from '../../theme/themes';

// The Settings log view (the SettingsGeneralLogs board) and the
// Session logs row on General. The words and numbers they show, the
// day headings, and each saved line drawn in its own SGR colors with
// your matches marked, the way the find bar marks them.

/** How many lines one page of the log view loads. */
export const LOG_PAGE_SIZE = 500;

const NUMBER = new Intl.NumberFormat('en-US');
const MONTHS = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
];

/** A count with thousands separators, like `708,350`. */
export function formatCount(n: number): string {
  return NUMBER.format(n);
}

const plural = (n: number, one: string, many: string) =>
  `${formatCount(n)} ${n === 1 ? one : many}`;

/** The Session logs row on General, like `447 logs and 708,350 lines
 *  on this Mac.` A log is one connection, so a session that connects
 *  three times saves three (Q21). `place` names the computer: Mac,
 *  PC, or computer. */
export function savedLogsText(logs: number, lines: number, place: string): string {
  if (logs === 0) return `Vosh has not saved a log on this ${place} yet.`;
  return `${plural(logs, 'log', 'logs')} and ${plural(lines, 'line', 'lines')} on this ${place}.`;
}

/** The count beside the pattern: `Newest 500 of 2,423 lines` while
 *  older matches wait to load, else how many there are. */
export function logCountText(loaded: number, total: number | null, pattern: string): string {
  const all = total ?? loaded;
  if (all === 0) return pattern ? 'No lines match' : 'No saved lines';
  if (loaded < all) return `Newest ${formatCount(loaded)} of ${formatCount(all)} lines`;
  return plural(all, 'line', 'lines');
}

/** The time column: the hour without a leading zero and the minute,
 *  on the 24 hour clock, like `3:52` or `17:28`. */
export function logTime(ms: number): string {
  const d = new Date(ms);
  return `${d.getHours()}:${String(d.getMinutes()).padStart(2, '0')}`;
}

/** A local calendar day as `yyyy-mm-dd`, for grouping. */
export function logDayKey(ms: number): string {
  const d = new Date(ms);
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${d.getFullYear()}-${mm}-${dd}`;
}

/** A day heading: `Today`, `September 26`, or `September 26, 2025`
 *  for another year. */
export function logDay(ms: number, now: number = Date.now()): string {
  if (logDayKey(ms) === logDayKey(now)) return 'Today';
  const d = new Date(ms);
  const day = `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  return d.getFullYear() === new Date(now).getFullYear() ? day : `${day}, ${d.getFullYear()}`;
}

/** A session in the scope select, by when it started: `Today, 3:52`
 *  or `September 28, 17:28`. */
export function logSessionLabel(startedMs: number, now: number = Date.now()): string {
  return `${logDay(startedMs, now)}, ${logTime(startedMs)}`;
}

// ── What the view reads ────────────────────────────────────────────

/** The spans of time the view reads (D35). Last 7 days opens the view. */
export type LogRange = 'session' | 'week' | 'month' | 'all';

export const LOG_RANGES: readonly { value: LogRange; label: string }[] = [
  { value: 'session', label: 'This session' },
  { value: 'week', label: 'Last 7 days' },
  { value: 'month', label: 'Last 30 days' },
  { value: 'all', label: 'All time' },
];

const DAY_MS = 86_400_000;

/** The scope of a range on one world: its host and port, and for the
 *  last 7 or 30 days the time the span starts. */
export function logRangeScope(
  range: LogRange,
  world: { host: string; port: number },
  now: number = Date.now(),
): LogScope {
  const scope: LogScope = { host: world.host, port: world.port };
  if (range === 'session') scope.thisSession = true;
  if (range === 'week') scope.sinceMs = now - 7 * DAY_MS;
  if (range === 'month') scope.sinceMs = now - 30 * DAY_MS;
  return scope;
}

/** The search field's placeholder for what the view reads. */
export function logPlaceholder(range: LogRange | null): string {
  switch (range) {
    case null:
      return 'Search this log';
    case 'session':
      return 'Search this session';
    case 'week':
      return 'Search the last 7 days';
    case 'month':
      return 'Search the last 30 days';
    case 'all':
      return 'Search every log';
  }
}

/** The name Save as file gives what the view reads, before its
 *  extension: `Vosh log, last 7 days`, or for one log the day and time
 *  it started, `Vosh log, 2026-10-08 17.28`. */
export function logFileName(range: LogRange | null, startedMs: number | null): string {
  if (range === null) {
    const at = startedMs ?? 0;
    const d = new Date(at);
    const time = `${String(d.getHours()).padStart(2, '0')}.${String(d.getMinutes()).padStart(2, '0')}`;
    return `Vosh log, ${logDayKey(at)} ${time}`;
  }
  const label = LOG_RANGES.find((r) => r.value === range)?.label ?? '';
  return `Vosh log, ${label.toLowerCase()}`;
}

/** What the view says when it reads no line and you typed no pattern. */
export function logEmptyText(range: LogRange | null): string {
  switch (range) {
    case null:
      return 'This log has no saved lines.';
    case 'session':
      return 'This session has saved nothing since Vosh opened.';
    case 'week':
      return 'Nothing saved from this world in the last 7 days.';
    case 'month':
      return 'Nothing saved from this world in the last 30 days.';
    case 'all':
      return 'Vosh saves every line as you play. It has none saved from this world yet.';
  }
}

export interface LogDayGroup<T> {
  key: string;
  day: string;
  lines: T[];
}

/** Lines under their day headings, in the order they come. */
export function groupLogDays<T extends { ts_ms: number }>(
  lines: readonly T[],
  now: number = Date.now(),
): LogDayGroup<T>[] {
  const groups: LogDayGroup<T>[] = [];
  for (const line of lines) {
    const key = logDayKey(line.ts_ms);
    const last = groups[groups.length - 1];
    if (last && last.key === key) last.lines.push(line);
    else groups.push({ key, day: logDay(line.ts_ms, now), lines: [line] });
  }
  return groups;
}

// ── A saved line in its own colors ──────────────────────────────────

/** An ANSI index (0 to 255) or a CSS color from a 24 bit SGR. */
export type LogColor = number | string;

export interface LogSpan {
  text: string;
  fg?: LogColor;
  bg?: LogColor;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  inverse?: boolean;
}

type LogStyle = Omit<LogSpan, 'text'>;

function applySgr(prev: LogStyle, codes: number[]): LogStyle {
  const next: LogStyle = { ...prev };
  for (let i = 0; i < codes.length; i += 1) {
    const code = codes[i];
    if (code === 0) {
      for (const key of Object.keys(next)) delete next[key as keyof LogStyle];
    } else if (code === 1) next.bold = true;
    else if (code === 3) next.italic = true;
    else if (code === 4) next.underline = true;
    else if (code === 7) next.inverse = true;
    else if (code === 22) next.bold = false;
    else if (code === 23) next.italic = false;
    else if (code === 24) next.underline = false;
    else if (code === 27) next.inverse = false;
    else if (code >= 30 && code <= 37) next.fg = code - 30;
    else if (code === 39) delete next.fg;
    else if (code >= 40 && code <= 47) next.bg = code - 40;
    else if (code === 49) delete next.bg;
    else if (code >= 90 && code <= 97) next.fg = code - 90 + 8;
    else if (code >= 100 && code <= 107) next.bg = code - 100 + 8;
    else if (code === 38 || code === 48) {
      const key = code === 38 ? 'fg' : 'bg';
      const mode = codes[i + 1];
      if (mode === 5 && codes[i + 2] !== undefined) {
        next[key] = Math.max(0, Math.min(255, codes[i + 2]));
        i += 2;
      } else if (mode === 2 && codes[i + 4] !== undefined) {
        const [r, g, b] = [codes[i + 2], codes[i + 3], codes[i + 4]].map((c) =>
          Math.max(0, Math.min(255, c)),
        );
        next[key] = `rgb(${r}, ${g}, ${b})`;
        i += 4;
      }
    }
  }
  return next;
}

const decoder = new TextDecoder('utf-8', { fatal: false });

/** Read a saved line's bytes into styled spans. SGR sets the style.
 *  Other escapes and control characters (tab aside) drop out, so the
 *  spans read the way the plain text column does. */
export function parseLogLine(raw: Uint8Array | string): LogSpan[] {
  const text = typeof raw === 'string' ? raw : decoder.decode(raw);
  const spans: LogSpan[] = [];
  let style: LogStyle = {};
  let buf = '';
  const flush = () => {
    if (buf) spans.push({ text: buf, ...style });
    buf = '';
  };
  let i = 0;
  while (i < text.length) {
    const ch = text[i];
    if (ch === '\x1b') {
      if (text[i + 1] === '[') {
        let j = i + 2;
        while (j < text.length && !(text[j] >= '@' && text[j] <= '~')) j += 1;
        if (text[j] === 'm') {
          flush();
          const params = text.slice(i + 2, j);
          const codes =
            params === '' ? [0] : params.split(';').map((s) => (s === '' ? 0 : Number(s)));
          style = applySgr(
            style,
            codes.filter((n) => Number.isFinite(n)),
          );
        }
        i = j + 1;
      } else {
        // A short escape. A character set pick (ESC ( B) is three
        // bytes, the rest two. Drop it.
        i += '()*+#'.includes(text[i + 1] ?? '') ? 3 : 2;
      }
      continue;
    }
    const code = ch.charCodeAt(0);
    if (ch === '\t' || code >= 0x20) buf += ch;
    i += 1;
  }
  flush();
  return spans;
}

/** The 16 ANSI colors a saved line reads with: the theme's own when
 *  you use the theme's colors for MUD text, else the base palette. */
export function logPalette(
  theme: XtermPalette,
  themeColors: boolean,
  base: readonly string[] | null,
): string[] {
  return ANSI_SLOTS.map((slot, i) =>
    themeColors ? theme[slot] : (base?.[i] ?? CANONICAL_ANSI_16[slot]),
  );
}

/** A span color as CSS. 0 to 15 read from the palette, 16 to 231 are
 *  the 6×6×6 cube, and 232 to 255 the gray ramp, as xterm draws them. */
export function logColorCss(color: LogColor, palette: readonly string[]): string {
  if (typeof color === 'string') return color;
  if (color < 16) return palette[color] ?? CANONICAL_ANSI_16[ANSI_SLOTS[color]];
  const { r, g, b } = indexedRgb(color, palette);
  return `rgb(${r}, ${g}, ${b})`;
}

export interface LogPieceCss {
  color?: string;
  background?: string;
  fontWeight?: number;
  fontStyle?: 'italic';
  textDecoration?: 'underline';
}

/** The CSS for a styled span, the way the terminal draws it. Bold
 *  lifts ANSI 0 to 7 to their bright pair, as MUDs mean it. A bright
 *  color takes the bold face only when you draw bright text in bold,
 *  and bold on any other color always does. Inverse swaps the colors,
 *  standing in the page's own for a side the line leaves unset. */
export function logSpanCss(
  span: Omit<LogSpan, 'text'>,
  palette: readonly string[],
  brightBold: boolean,
): LogPieceCss {
  let fg = span.fg;
  if (span.bold && typeof fg === 'number' && fg < 8) fg += 8;
  const bright = typeof fg === 'number' && fg >= 8 && fg < 16;
  const css: LogPieceCss = {};
  const fgCss = fg === undefined ? undefined : logColorCss(fg, palette);
  const bgCss = span.bg === undefined ? undefined : logColorCss(span.bg, palette);
  if (span.inverse) {
    css.color = bgCss ?? 'var(--bg)';
    css.background = fgCss ?? 'var(--text)';
  } else {
    if (fgCss) css.color = fgCss;
    if (bgCss) css.background = bgCss;
  }
  if (bright ? brightBold : span.bold) css.fontWeight = 700;
  if (span.italic) css.fontStyle = 'italic';
  if (span.underline) css.textDecoration = 'underline';
  return css;
}

// ── Matches ─────────────────────────────────────────────────────────

/** A JavaScript reading of the pattern for marking matches. The
 *  backend searches with Rust's regex engine. A pattern JavaScript
 *  reads differently only loses its marks, never its lines. */
export function logMatcher(pattern: string, caseSensitive: boolean): RegExp | null {
  if (!pattern) return null;
  const flags = caseSensitive ? 'g' : 'gi';
  for (const extra of ['u', '']) {
    try {
      return new RegExp(pattern, flags + extra);
    } catch {
      // Try without Unicode mode, which rejects some escapes.
    }
  }
  return null;
}

/** Where `matcher` matches `text`, as [start, end) pairs. Empty
 *  matches mark nothing. */
export function matchRanges(text: string, matcher: RegExp | null): [number, number][] {
  if (!matcher) return [];
  const re = new RegExp(
    matcher.source,
    matcher.flags.includes('g') ? matcher.flags : `${matcher.flags}g`,
  );
  const ranges: [number, number][] = [];
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    if (m[0].length === 0) {
      re.lastIndex += 1;
      continue;
    }
    ranges.push([m.index, m.index + m[0].length]);
  }
  return ranges;
}

export interface LogPiece extends LogSpan {
  match: boolean;
}

/** Split spans where matches start and end, and mark the matched
 *  pieces. */
export function markMatches(spans: readonly LogSpan[], matcher: RegExp | null): LogPiece[] {
  const ranges = matchRanges(spans.map((s) => s.text).join(''), matcher);
  const pieces: LogPiece[] = [];
  let offset = 0;
  let r = 0;
  for (const span of spans) {
    const end = offset + span.text.length;
    let at = offset;
    while (at < end) {
      while (r < ranges.length && ranges[r][1] <= at) r += 1;
      const range = ranges[r];
      let stop: number;
      let match: boolean;
      if (range && range[0] <= at) {
        stop = Math.min(range[1], end);
        match = true;
      } else {
        stop = range ? Math.min(range[0], end) : end;
        match = false;
      }
      pieces.push({ ...span, text: span.text.slice(at - offset, stop - offset), match });
      at = stop;
    }
    offset = end;
  }
  return pieces;
}
