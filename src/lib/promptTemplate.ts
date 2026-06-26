// ANSI renderer for the custom prompt template. Takes the user's
// template string plus the var bag emitted by the prompt-vars
// trigger and returns a byte sequence ready for term.write().
//
// Token grammar reuses vitalsTemplate.ts so users get the same
// `%name`, `%{name}`, and `%%` semantics they already know.
// Renderable token kinds:
//
//   %name               — plain text substitution. Unknown vars
//                         render as the raw `%name` literal so the
//                         user spots their typo immediately.
//   %name_bar           — auto-colored bar 10 cells wide.
//   %name_bar:W         — same, width W cells.
//   %name_bar:W:COLOR   — same, fixed color. `auto` (default)
//                         picks green / yellow / red by percent.
//                         Named colors: green, red, yellow, blue,
//                         cyan, magenta, white.
//   %pct_name           — current/max as an integer percent.
//   %c_<spec>           — set the text color until reset. The spec is a
//   %{c:<spec>}           named color (red green yellow blue cyan magenta
//                         white gray), a 256 index (`%{c:196}`), truecolor
//                         (`%{c:255,128,0}` or `%{c:#ff8800}`), or a stat
//                         name to auto-color by its percent (`%c_hp`). The
//                         `%{...}` form is required for the colon/comma
//                         syntaxes; `%c_red` and `%c_196` also work bare.
//   %c_reset            — clear color back to the default.
//   %time               — current local time, HH:MM:SS.
//   %date               — current local date, YYYY-MM-DD.
//
// `name` is the current value; `mname` (or fallback `max_name`,
// `name_max`, `maxname`) is the cap.
//
// No backend involvement. The caller wires this into the
// prompt-vars listener and writes the result to xterm; the prompt
// trigger pipeline already gags the original bytes.

import { tokenizeTemplate } from './vitalsTemplate';

export type PromptVars = Record<string, string>;

const RESET = '\x1b[0m';

const COLOR_FG: Record<string, string> = {
  green: '\x1b[38;5;42m',
  red: '\x1b[38;5;196m',
  yellow: '\x1b[38;5;220m',
  blue: '\x1b[38;5;39m',
  cyan: '\x1b[38;5;51m',
  magenta: '\x1b[38;5;201m',
  white: '\x1b[38;5;255m',
  gray: '\x1b[38;5;240m',
};

const EMPTY_FG = COLOR_FG.gray;

function parseNumber(value: string | undefined): number | null {
  if (value === undefined || value === null) return null;
  const trimmed = value.trim();
  if (trimmed.length === 0) return null;
  const n = Number(trimmed);
  return Number.isFinite(n) ? n : null;
}

function lookupMax(vars: PromptVars, name: string): number | null {
  const candidates = [`m${name}`, `${name}_max`, `max_${name}`, `max${name}`];
  for (const key of candidates) {
    const n = parseNumber(vars[key]);
    if (n !== null) return n;
  }
  return null;
}

function colorForPercent(pct: number): string {
  if (pct >= 0.66) return COLOR_FG.green;
  if (pct >= 0.33) return COLOR_FG.yellow;
  return COLOR_FG.red;
}

function renderBar(value: number, max: number, width: number, color: string): string {
  if (!Number.isFinite(width) || width < 1) width = 10;
  if (max <= 0) {
    return EMPTY_FG + '░'.repeat(width) + RESET;
  }
  const pct = Math.max(0, Math.min(1, value / max));
  const filled = Math.round(pct * width);
  const empty = width - filled;
  const fg = color === 'auto' ? colorForPercent(pct) : (COLOR_FG[color] ?? COLOR_FG.green);
  return fg + '█'.repeat(filled) + EMPTY_FG + '░'.repeat(empty) + RESET;
}

function baseFromBarName(name: string): string | null {
  if (name.endsWith('_bar')) {
    const base = name.slice(0, -4);
    return base.length > 0 ? base : null;
  }
  if (name.startsWith('bar_')) {
    const base = name.slice(4);
    return base.length > 0 ? base : null;
  }
  return null;
}

// Consume `:width:color` parameters from the start of the given
// text. Returns the parsed values plus the leftover text. Both
// parameters are optional and either can be omitted (e.g. `:20`
// for width only, `::green` for color only — though that form is
// awkward; users are more likely to write `:20:green` or nothing).
function consumeBarParams(tail: string): {
  width: number;
  color: string;
  rest: string;
} {
  const result = { width: 10, color: 'auto', rest: tail };
  if (!tail.startsWith(':')) return result;
  const match = /^:([0-9]*)(?::([a-z]+))?/i.exec(tail);
  if (!match) return result;
  if (match[1] && match[1].length > 0) {
    const w = Number(match[1]);
    if (Number.isFinite(w) && w > 0) result.width = Math.min(80, Math.floor(w));
  }
  if (match[2]) result.color = match[2].toLowerCase();
  result.rest = tail.slice(match[0].length);
  return result;
}

// Resolve a color spec to an SGR sequence. Accepts a named color, `reset`,
// a 256-palette index (`196`), truecolor `r,g,b` or `#rrggbb`/`rrggbb`, or a
// stat name (auto-color by its percent). Returns null when unrecognized.
function colorCodeFromSpec(spec: string, vars: PromptVars): string | null {
  if (spec === 'reset') return RESET;
  if (COLOR_FG[spec]) return COLOR_FG[spec];
  if (/^\d{1,3}$/.test(spec)) {
    const n = Number(spec);
    if (n >= 0 && n <= 255) return `\x1b[38;5;${n}m`;
  }
  const hex = /^#?([0-9a-f]{6})$/i.exec(spec);
  if (hex) {
    const r = parseInt(hex[1].slice(0, 2), 16);
    const g = parseInt(hex[1].slice(2, 4), 16);
    const b = parseInt(hex[1].slice(4, 6), 16);
    return `\x1b[38;2;${r};${g};${b}m`;
  }
  const rgb = /^(\d{1,3}),(\d{1,3}),(\d{1,3})$/.exec(spec);
  if (rgb) {
    const clamp = (s: string) => Math.min(255, Math.max(0, Number(s)));
    return `\x1b[38;2;${clamp(rgb[1])};${clamp(rgb[2])};${clamp(rgb[3])}m`;
  }
  const value = parseNumber(vars[spec]);
  const max = lookupMax(vars, spec);
  if (value !== null && max !== null && max > 0) {
    return colorForPercent(Math.max(0, Math.min(1, value / max)));
  }
  return null;
}

function pad2(n: number): string {
  return n < 10 ? `0${n}` : `${n}`;
}

function currentTime(): string {
  const d = new Date();
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
}

function currentDate(): string {
  const d = new Date();
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

function renderPlainToken(name: string, raw: string, vars: PromptVars): string {
  // Color directives. `%c_reset` / `%{c:reset}` clears. The spec after
  // `c_` or `c:` may be a named color, a 256 index, `r,g,b`, `#rrggbb`, or
  // a stat name (auto-color by its percent). E.g. `%{c:255,128,0}`,
  // `%c_196`, `%c_hp%hp/%maxhp%c_reset`.
  if (name.startsWith('c_') || name.startsWith('c:')) {
    return colorCodeFromSpec(name.slice(2), vars) ?? raw;
  }
  if (name === 'time') return currentTime();
  if (name === 'date') return currentDate();
  if (name.startsWith('pct_')) {
    const base = name.slice(4);
    const value = parseNumber(vars[base]);
    const max = lookupMax(vars, base);
    if (value !== null && max !== null && max > 0) {
      return `${Math.round((value / max) * 100)}`;
    }
    return raw;
  }
  const direct = vars[name];
  if (direct !== undefined) return direct;
  return raw;
}

export function renderPromptTemplate(template: string, vars: PromptVars): string {
  if (template.length === 0) return '';
  const segments = tokenizeTemplate(template);
  let out = '';
  for (let i = 0; i < segments.length; i += 1) {
    const segment = segments[i];
    if (segment.kind === 'text') {
      out += segment.text;
      continue;
    }
    const barBase = baseFromBarName(segment.name);
    if (barBase) {
      // The tokenizer stops at non-alphanumeric, so any
      // `:width:color` parameters landed in the next text segment.
      // Peek at it, consume the matched prefix, and put the rest
      // back in the output stream.
      const next = segments[i + 1];
      let width = 10;
      let color = 'auto';
      if (next && next.kind === 'text') {
        const parsed = consumeBarParams(next.text);
        width = parsed.width;
        color = parsed.color;
        segments[i + 1] = { kind: 'text', text: parsed.rest };
      }
      const value = parseNumber(vars[barBase]);
      const max = lookupMax(vars, barBase);
      if (value !== null && max !== null) {
        out += renderBar(value, max, width, color);
      } else {
        out += segment.raw;
      }
      continue;
    }
    out += renderPlainToken(segment.name, segment.raw, vars);
  }
  // Always reset-terminate so an unclosed color (e.g. a template still
  // being typed, before `%c_reset` is added) cannot bleed into the
  // server output that follows the prompt.
  return out.length > 0 ? out + RESET : out;
}
