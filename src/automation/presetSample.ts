// A preset's sample, drawn as the terminal draws it. Get started and the
// Looks like row of a preset card show each line of the sample through the
// preset's own triggers, in the colors of your theme, and presets.test.ts
// checks every color the model paints.

import type { HighlightStyle, TriggerTarget } from '../ipc/automation';
import { appearanceOf } from '../theme/chrome';
import { indexedRgb, liftAtHue, parseHex, toHex, type Rgb } from '../theme/color';
import { type Preset, type PresetSampleLine, presetTriggers } from './presets';

// What the trigger engine draws on a line, modeled on process_on_ground in
// crates/automation/src/trigger/engine.rs. Triggers run high priority
// first. A Replace rewrites the text through its template, $1 to $9
// filled from the groups, a highlight colors the text it matches with the
// first span winning, and a base color fills what is left in the default
// color. The runs name each color by its ANSI name, its 256 color index
// or its hex, with bold before it. The model draws no highlight over a
// replaced line, which the engine matches against the rebuilt text, since
// no sample has one.
/** A stretch of the drawn line in one color: its ANSI name, its 256
 *  color index or its hex, with bold before it, or null for the default
 *  color. */
export type SampleRun = [text: string, color: string | null];

/** What the triggers did to a line: the names that fired, the line as
 *  runs, and the panes it went to. */
export interface DrawnSample {
  fired: string[];
  runs: SampleRun[];
  routes: string[];
}

const ANSI_NAMES = ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white'];

function styleColor(style: HighlightStyle): string | null {
  if (!style.fg) return null;
  return style.bold ? `bold ${style.fg}` : style.fg;
}

// Each character of `text`, which holds SGR codes, with the color it
// shows in.
function sgrChars(text: string): { ch: string; color: string | null }[] {
  const chars: { ch: string; color: string | null }[] = [];
  let fg: string | null = null;
  let bold = false;
  // eslint-disable-next-line no-control-regex
  for (const m of text.matchAll(/\x1b\[([0-9;]*)m|([^\x1b])/g)) {
    if (m[2] !== undefined) {
      chars.push({ ch: m[2], color: fg && bold ? `bold ${fg}` : fg });
      continue;
    }
    const codes = m[1].split(';').map((c) => (c === '' ? 0 : Number(c)));
    for (let i = 0; i < codes.length; i++) {
      const c = codes[i];
      if (c === 0) [fg, bold] = [null, false];
      else if (c === 1) bold = true;
      else if (c === 22) bold = false;
      else if (c === 39) fg = null;
      else if (c >= 30 && c <= 37) fg = ANSI_NAMES[c - 30];
      else if (c >= 90 && c <= 97) fg = `bright_${ANSI_NAMES[c - 90]}`;
      else if (c === 38 && codes[i + 1] === 5) {
        fg = String(codes[i + 2]);
        i += 2;
      } else if (c === 38 && codes[i + 1] === 2) {
        fg = `#${codes
          .slice(i + 2, i + 5)
          .map((n) => n.toString(16).padStart(2, '0'))
          .join('')}`;
        i += 4;
      }
    }
  }
  return chars;
}

/** Draw a line of a preset's sample through the preset's own triggers,
 *  each color key filled from `colors`, yours by key, over the preset's
 *  own. */
export function drawSample(
  preset: Preset,
  line: PresetSampleLine,
  colors: Readonly<Record<string, string>> = {},
): DrawnSample {
  const scope: TriggerTarget = line.target ?? 'line';
  // Which triggers see the line, as MatchScope::matches has it.
  const reaches = (target: TriggerTarget = 'line') =>
    target === 'line' ||
    (target === 'room' && scope !== 'line') ||
    (target === 'room_target' && scope === 'room_target');
  const triggers = presetTriggers(preset, colors)
    .map((t, n) => ({ t, n }))
    .sort((a, b) => b.t.priority - a.t.priority || a.n - b.n)
    .map(({ t }) => t);
  const fired: string[] = [];
  const routes: string[] = [];
  const spans: [RegExp, string | null][] = [];
  let base: string | null = null;
  let text = line.text;
  let replaced = false;
  for (const t of triggers) {
    if (!t.enabled || !reaches(t.target)) continue;
    for (const row of t.patterns) {
      const regex = new RegExp(row.pattern);
      if (!row.enabled || !regex.test(line.text)) continue;
      if (!fired.includes(t.name)) fired.push(t.name);
      const groups = (new RegExp(`${row.pattern}|`).exec('')?.length ?? 1) - 1;
      for (const action of t.actions) {
        if (action.kind === 'replace') {
          text = text.replace(new RegExp(row.pattern, 'g'), (...m: unknown[]) =>
            action.template.replace(/\$(\d)/g, (_, n: string) => {
              const at = Number(n);
              return at <= groups ? String(m[at] ?? '') : '';
            }),
          );
          replaced = true;
        } else if (action.kind === 'highlight' && action.style.base) {
          base ??= styleColor(action.style);
        } else if (action.kind === 'highlight') {
          spans.push([regex, styleColor(action.style)]);
        } else if (action.kind === 'route' && !routes.includes(action.pane)) {
          routes.push(action.pane);
        }
      }
    }
  }
  if (replaced && spans.length > 0) {
    throw new Error(`the model draws no highlight over a replaced line: ${line.text}`);
  }
  const chars = replaced
    ? sgrChars(text)
    : [...line.text].map((ch) => ({ ch, color: null as string | null }));
  const taken = chars.map(() => false);
  for (const [regex, color] of spans) {
    if (!color) continue;
    for (const m of line.text.matchAll(new RegExp(regex.source, 'g'))) {
      const start = m.index;
      const end = start + m[0].length;
      if (end === start || taken.slice(start, end).some(Boolean)) continue;
      for (let i = start; i < end; i++) {
        taken[i] = true;
        chars[i].color = color;
      }
    }
  }
  const runs: SampleRun[] = [];
  for (const { ch, color } of chars) {
    const shown = color ?? base;
    const last = runs.at(-1);
    if (last && last[1] === shown) last[0] += ch;
    else runs.push([ch, shown]);
  }
  return { fired, runs, routes };
}

/** The contrast Keep highlight colors readable lifts a fixed color to,
 *  READABLE_CONTRAST in crates/automation/src/trigger/readable.rs. */
const READABLE_CONTRAST = 4.5;

/** What a run reads its color from: the terminal's sixteen, the ground a
 *  fixed color lifts against while Keep highlight colors readable is on,
 *  else null, and whether bright text draws bold. */
export interface SamplePaint {
  palette: readonly string[];
  ground: string | null;
  brightBold: boolean;
}

/** A run's color as CSS and whether it draws bold. Bold lifts ANSI 0 to
 *  7 to their bright pair, as MUDs mean it, and a bright color takes the
 *  bold face only when you draw bright text in bold. A 256 color past the
 *  16 or a true color moves in lightness alone until it reads on the
 *  ground, as lift_to_contrast in readable.rs moves it, so on a light
 *  theme it darkens as it does in the terminal. */
export function sampleRunCss(
  color: string | null,
  paint: SamplePaint,
): { color?: string; bold: boolean } {
  if (color === null) return { bold: false };
  const bold = color.startsWith('bold ');
  const name = bold ? color.slice(5) : color;
  const bright = name.startsWith('bright_');
  const base = ANSI_NAMES.indexOf(bright ? name.slice(7) : name);
  const index = base >= 0 ? base + (bright ? 8 : 0) : /^\d+$/.test(name) ? Number(name) : -1;
  if (index >= 0 && index < 16) {
    const shown = bold && index < 8 ? index + 8 : index;
    return { color: paint.palette[shown], bold: shown >= 8 ? paint.brightBold : bold };
  }
  const rgb = index >= 0 ? indexedRgb(index, paint.palette) : parseHex(name);
  if (!rgb) return { bold };
  return { color: toHex(readable(rgb, paint.ground)), bold };
}

function readable(rgb: Rgb, ground: string | null): Rgb {
  const bg = ground === null ? null : parseHex(ground);
  if (!bg) return rgb;
  return liftAtHue(rgb, bg, READABLE_CONTRAST, appearanceOf(bg) === 'dark' ? 1 : -1);
}

/** Where the words a line quotes sit, start and end, or null. A tell
 *  quotes what a character says, and a page draws those words as a bar,
 *  never as text. */
export function quotedWords(text: string): [number, number] | null {
  const m = /\s'([^']+)'$/.exec(text);
  if (!m) return null;
  const start = m.index + 2;
  return [start, start + m[1].length];
}

/** Where a sample line draws bars, start and end, in order: the words a
 *  tell quotes and each of `bars` where it first shows in `text`. */
export function sampleBars(text: string, bars: readonly string[] = []): [number, number][] {
  const spans = bars
    .map((words): [number, number] => {
      const at = text.indexOf(words);
      return [at, at + words.length];
    })
    .filter(([at]) => at >= 0);
  const quoted = quotedWords(text);
  if (quoted) spans.push(quoted);
  return spans.sort((a, b) => a[0] - b[0]);
}
