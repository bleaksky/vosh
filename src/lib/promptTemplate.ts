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

function renderPlainToken(name: string, raw: string, vars: PromptVars): string {
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
  return out;
}
