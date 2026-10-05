// The game color fitter. It measures a terminal palette against the 46
// checks Aabahran asks of one (T1 to T7, for body text, sentence and
// cue colors, room names, bold twins, and red kept apart from green and
// yellow for a color blind player), and fits a palette to them with the
// smallest visible change. Every slot moves only in OKLCH lightness at
// its own hue, and chroma gives way only where sRGB runs out, so a red
// stays the scheme's red.
//
// A port of the Themes review's metrics (mud.mjs and fit.mjs). The
// arithmetic and its order stay as the review wrote them, so a fit here
// gives the colors the review measured, slot for slot.
//
// Where a fit comes from. A built in theme ships its fit in themes.ts
// (fitted), worked out ahead by this file, and gameFit.test.ts fits
// each one again when you run it with VOSH_FIT_THEMES=1. Settings fits
// a custom theme when you import, copy or change it, and when
// Appearance opens on one that keeps no fit, and keeps the fit in your
// config (settings/pages/appearance/fitAndKeep). The main window fits a
// custom theme in play that keeps no fit when it loads your config, and
// holds that fit in memory only (customThemeFits, holdFit in themes.ts).
// Every fit runs in a worker (fitOffThread, gameFit.worker). Play draws
// the fit while Fit game colors is on (playPalette in themes.ts,
// fitGameColors).
//
// A color vision other than Typical gets fits of its own. Each starts
// from the Typical fit and moves the colors of the cue pairs the vision
// confuses until, seen through that vision, each pair stands as far
// apart as a typical eye sees it in the Typical fit (visionChecks). It
// moves lightness first, and turns a hue a little only where lightness
// leaves a pair short (HUE_TURN). No color it moves runs into body text,
// white or bold white (textGuards), and a fit that moves no color far
// enough to see keeps the Typical fit (VISIBLE_CHANGE). A built in theme
// ships them in themes.ts (VISION_FITS) where the fit for a vision moves
// anything, and gameFit.test.ts fits them again with VOSH_FIT_THEMES=1.

import { indexedRgb } from './bandCells';
import { ANSI_SLOTS, type AnsiSlot } from './baseAnsi';
import {
  contrast,
  deltaEOk,
  linearToOklab,
  linearToRgb,
  oklchToRgbInGamut,
  parseHex,
  rgbToLinear,
  rgbToOklab,
  rgbToOklch,
  toHex,
  type Oklch,
  type Rgb,
} from './color';
import type { XtermPalette } from './themes';

/** The slots the fit may move, body text and the 16 ANSI colors. */
type GameSlot = 'foreground' | AnsiSlot;
export const GAME_SLOTS: readonly GameSlot[] = ['foreground', ...ANSI_SLOTS];

/** One check, with its id, the measured value to one decimal, the
 *  target as `>=75` or a range `45..60`, and whether the palette meets
 *  it. */
export interface GameCheck {
  id: string;
  value: number;
  need: string;
  ok: boolean;
}

const rgb = (hex: string): Rgb => {
  const c = parseHex(hex);
  if (!c) throw new Error(`not a hex color: ${hex}`);
  return c;
};

// ── APCA 0.0.98G-4g, with the SAPC constants ───────────────────────

const apcaY = (hex: string) => {
  const c = rgb(hex);
  return (
    0.2126729 * (c.r / 255) ** 2.4 + 0.7151522 * (c.g / 255) ** 2.4 + 0.072175 * (c.b / 255) ** 2.4
  );
};

/** APCA lightness contrast of `text` on `ground`, positive for dark
 *  text on light, negative for light text on dark. */
export function apca(text: string, ground: string): number {
  const clampY = (y: number) => (y < 0.022 ? y + (0.022 - y) ** 1.414 : y);
  const yt = clampY(apcaY(text));
  const yb = clampY(apcaY(ground));
  if (Math.abs(yb - yt) < 0.0005) return 0;
  let out;
  if (yb > yt) {
    const s = (yb ** 0.56 - yt ** 0.57) * 1.14;
    out = s < 0.1 ? 0 : s - 0.027;
  } else {
    const s = (yb ** 0.65 - yt ** 0.62) * 1.14;
    out = s > -0.1 ? 0 : s + 0.027;
  }
  return out * 100;
}

const lc = (text: string, ground: string) => Math.abs(apca(text, ground));
const wcag = (a: string, b: string) => contrast(rgb(a), rgb(b));

// ── OKLab distances, times 100 ─────────────────────────────────────

const lightness = (hex: string) => rgbToOklab(rgb(hex)).L * 100;
const dE = (a: string, b: string) => deltaEOk(rgb(a), rgb(b));
const dL = (a: string, b: string) => Math.abs(lightness(a) - lightness(b));

// ── Color vision deficiency: Machado, Oliveira and Fernandes 2009 at
// severity 1, applied in linear sRGB ─────────────────────────────────

type Cvd = 'protan' | 'deutan' | 'tritan';
type Matrix = readonly [readonly number[], readonly number[], readonly number[]];

/** The simulation matrices, by deficiency. The checks measure with
 *  them, and so does the theme gallery's Vision preview (seenBy). */
const MACHADO: Readonly<Record<Cvd, Matrix>> = {
  protan: [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
  ],
  deutan: [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.01182, 0.04294, 0.968881],
  ],
  tritan: [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.3039],
  ],
};

// The linear sRGB channels a color takes for `kind`.
function simulateRgb(c: Rgb, kind: Cvd): [number, number, number] {
  const v = rgbToLinear(c);
  const [r, g, b] = MACHADO[kind].map((row) =>
    Math.max(0, Math.min(1, row[0] * v[0] + row[1] * v[1] + row[2] * v[2])),
  );
  return [r, g, b];
}

const simulate = (hex: string, kind: Cvd) => simulateRgb(rgb(hex), kind);

function seenAs(hex: string, kind: Cvd) {
  return linearToOklab(simulate(hex, kind));
}

const dECvd = (a: string, b: string, kind: Cvd) => {
  const p = seenAs(a, kind);
  const q = seenAs(b, kind);
  return 100 * Math.hypot(p.L - q.L, p.a - q.a, p.b - q.b);
};

// ── Color vision ───────────────────────────────────────────────────

/** The color vision Vosh keeps the cue colors apart for, from UiConfig
 *  color_vision. Typical asks the 46 checks as they stand, and every
 *  fit for Typical is the one before color vision. Each other vision
 *  keeps the pairs visionPairs names as far apart, seen through it, as
 *  a typical eye sees them in the Typical fit, in the game text while
 *  Fit game colors is on and in the window's status colors (chrome
 *  deriveChrome). */
export type ColorVision = 'typical' | 'deuteranopia' | 'protanopia' | 'tritanopia';

export const COLOR_VISIONS: readonly ColorVision[] = [
  'typical',
  'deuteranopia',
  'protanopia',
  'tritanopia',
];

const CVD_OF: Record<ColorVision, Cvd | null> = {
  typical: null,
  deuteranopia: 'deutan',
  protanopia: 'protan',
  tritanopia: 'tritan',
};

/** The vision `value` names, Typical for anything else. */
export function toColorVision(value: unknown): ColorVision {
  return COLOR_VISIONS.find((v) => v === value) ?? 'typical';
}

/** The color `hex` shows a player with `vision`, through the matrices
 *  the T7 checks measure with. A color that is not hex, and any color
 *  under Typical, comes back as it is. */
export function seenBy(hex: string, vision: ColorVision): string {
  const kind = CVD_OF[vision];
  if (!kind || !parseHex(hex)) return hex;
  return toHex(linearToRgb(simulate(hex, kind)));
}

/** The OKLab coordinates of `c` as a player with `vision` sees it,
 *  through the same matrices. Under Typical, its own. */
export function seenLab(c: Rgb, vision: ColorVision): { L: number; a: number; b: number } {
  const kind = CVD_OF[vision];
  return kind ? linearToOklab(simulateRgb(c, kind)) : rgbToOklab(c);
}

/** How far apart a player with `vision` sees two colors, in OKLab dE
 *  times 100. Under Typical it is the straight OKLab distance. */
export function seenApart(a: Rgb, b: Rgb, vision: ColorVision): number {
  if (!CVD_OF[vision]) return deltaEOk(a, b);
  const p = seenLab(a, vision);
  const q = seenLab(b, vision);
  return 100 * Math.hypot(p.L - q.L, p.a - q.a, p.b - q.b);
}

/** How far a fit for a color vision may turn a cue color's hue, in
 *  degrees of OKLCH hue. A fit turns a hue only where lightness alone
 *  leaves a pair short of its target, and only as far as the target
 *  asks, up to HUE_TURN. Where HUE_TURN still leaves a pair short it
 *  may go on to HUE_TURN_FAR. Neither takes a color past HUE_LIMIT. The
 *  window's status colors turn by the same bounds (chrome deriveChrome). */
export const HUE_TURN = 30;
export const HUE_TURN_FAR = 40;

/** The family of each color a fit may turn, and the OKLCH hue it never
 *  turns past, so it still reads as its own color to a typical eye. Red
 *  turns toward orange and stops at 33, past tomato at 32 and short of
 *  orange red at 35. Green turns toward teal and stops at 165, past
 *  medium spring green at 157 and short of aquamarine at 169. Cyan
 *  turns toward blue and stops at 240, short of dodger blue at 253. A
 *  color already at or past its limit keeps its hue. */
export type TurnFamily = 'red' | 'green' | 'cyan';
export const HUE_LIMIT: Readonly<Record<TurnFamily, number>> = { red: 33, green: 165, cyan: 240 };

/** How far a color of `family` at OKLCH hue `hue` may turn under
 *  `bound` degrees. */
export function turnRoom(family: TurnFamily, hue: number, bound: number): number {
  return Math.max(0, Math.min(bound, HUE_LIMIT[family] - hue));
}

/** The color `c` turned `degrees` up OKLCH hue at its own lightness and
 *  chroma, giving up chroma only where sRGB runs out, and rounded. */
export function turnHue(c: Rgb, degrees: number): Rgb {
  const lch = rgbToOklch(c);
  const out = oklchToRgbInGamut({ ...lch, h: (lch.h + degrees) % 360 });
  return { r: Math.round(out.r), g: Math.round(out.g), b: Math.round(out.b) };
}

// ── The fixed colors the game sends ────────────────────────────────

/** The color xterm draws for 256 color index `n`, 16 to 255. */
export function xterm256(n: number): string {
  const [r, g, b] = indexedRgb(n, []);
  return toHex({ r, g, b });
}

/** The 256 color indexes the game sends whatever the theme. 240 is the
 *  Wizi and Incog prompt prefix, the rest are minimap glyphs, and 213
 *  is your own @ on the minimap. */
export const GAME_FIXED_COLORS = [
  240, 249, 180, 77, 34, 143, 241, 75, 33, 58, 117, 220, 196, 255, 213,
] as const;

// ── The targets ────────────────────────────────────────────────────

const T = {
  fgLc: 75,
  fgWcag: 7,
  sentenceLc: 60,
  sentenceWcag: 4.5,
  cueLc: 45,
  dimMin: 45,
  dimMax: 60,
  dimBelowFg: 15,
  // Near pure black OKLab L runs so steep and APCA clips so early that
  // neither can say whether black text shows, so black stands off the
  // ground by WCAG contrast. 1.25 to 1 is #1e1e1e on #000000.
  blackWcag: 1.25,
  pairDE: 10,
  pairDL: 4,
  // Bold white stands farther from the ground than body text, so
  // weather, prayers and Lysenties read above it.
  fgBrightWhiteDL: 8,
  rySep: 10,
  rRbDL: 8,
  tritanCG: 8,
};

/** Colors the game writes whole sentences in, and colors it marks a
 *  cue with. */
const SENTENCE: readonly AnsiSlot[] = [
  'white',
  'yellow',
  'green',
  'cyan',
  'brightRed',
  'brightGreen',
  'brightYellow',
  'brightBlue',
  'brightMagenta',
  'brightCyan',
  'brightWhite',
];
const CUE: readonly AnsiSlot[] = ['red', 'blue', 'magenta'];

/** Each plain color and its bold twin. */
const PAIRS: readonly (readonly [AnsiSlot, AnsiSlot])[] = [
  ['red', 'brightRed'],
  ['green', 'brightGreen'],
  ['yellow', 'brightYellow'],
  ['blue', 'brightBlue'],
  ['magenta', 'brightMagenta'],
  ['cyan', 'brightCyan'],
  ['white', 'brightWhite'],
];

// The bold twin of each plain color, and the plain twin of each bold.
const TWIN: Partial<Record<GameSlot, AnsiSlot>> = Object.fromEntries(
  PAIRS.flatMap(([n, b]) => [
    [n, b],
    [b, n],
  ]),
);

/** The cue pairs a protanope and a deuteranope must still tell apart,
 *  each with its dE floor. */
const CVD_PAIRS: readonly (readonly [AnsiSlot, AnsiSlot, number])[] = [
  ['red', 'yellow', 12],
  ['red', 'brightYellow', 12],
  ['red', 'green', 10],
  ['brightRed', 'brightGreen', 10],
  ['yellow', 'green', 10],
];

const isDark = (p: XtermPalette) => rgbToOklab(rgb(p.background)).L < 0.6;
const away = (p: XtermPalette, hex: string) => Math.abs(lightness(hex) - lightness(p.background));

/** Every check, in the order the review lists them. */
export function checks(p: XtermPalette): GameCheck[] {
  const bg = p.background;
  const out: GameCheck[] = [];
  const add = (id: string, value: number, need: string, ok: boolean) =>
    out.push({ id, value: +value.toFixed(1), need, ok });
  const fgLc = lc(p.foreground, bg);
  add('T1 fg Lc', fgLc, '>=75', fgLc >= T.fgLc);
  add('T1 fg WCAG', wcag(p.foreground, bg), '>=7', wcag(p.foreground, bg) >= T.fgWcag);
  for (const k of SENTENCE) {
    add(`T2 ${k} Lc`, lc(p[k], bg), '>=60', lc(p[k], bg) >= T.sentenceLc);
  }
  for (const k of CUE) add(`T3 ${k} Lc`, lc(p[k], bg), '>=45', lc(p[k], bg) >= T.cueLc);
  const dim = lc(p.brightBlack, bg);
  const dimTop = Math.min(T.dimMax, fgLc - T.dimBelowFg);
  add(
    'T4 brightBlack Lc',
    dim,
    `45..${dimTop.toFixed(0)}`,
    dim >= T.dimMin && dim <= dimTop + 0.05,
  );
  add('T5 black WCAG', wcag(p.black, bg), '>=1.25', wcag(p.black, bg) >= T.blackWcag - 1e-9);
  for (const [n, b] of PAIRS) {
    add(`T6 ${n} pair dE`, dE(p[n], p[b]), '>=10', dE(p[n], p[b]) >= T.pairDE);
    const step = away(p, p[b]) - away(p, p[n]);
    add(`T6 ${n} bright step dL`, step, '>=4', step >= T.pairDL);
  }
  const fw = away(p, p.brightWhite) - away(p, p.foreground);
  add('T6 fg/brightWhite dL', fw, '>=8', fw >= T.fgBrightWhiteDL);
  for (const kind of ['protan', 'deutan'] as const) {
    for (const [a, b, need] of CVD_PAIRS) {
      const v = dECvd(p[a], p[b], kind);
      add(`T7 ${kind} ${a}/${b}`, v, `>=${need}`, v >= need);
    }
  }
  add('T7 red/yellow dL', dL(p.red, p.yellow), '>=10', dL(p.red, p.yellow) >= T.rySep);
  add('T7 red/brightRed dL', dL(p.red, p.brightRed), '>=8', dL(p.red, p.brightRed) >= T.rRbDL);
  const tc = dECvd(p.cyan, p.green, 'tritan');
  add('T7 tritan cyan/green', tc, '>=8', tc >= T.tritanCG);
  return out;
}

// ── The greedy tuner ───────────────────────────────────────────────

// Step a color in OKLCH lightness at its own hue until `ok` holds, or
// until lightness runs out. `dir` 1 is lighter, -1 darker.
function stepAtHue(hex: string, dir: number, ok: (hex: string) => boolean): string {
  if (ok(hex)) return hex;
  const lch = rgbToOklch(rgb(hex));
  let last = hex;
  for (let l = lch.L; l >= 0 && l <= 1.0001; l += dir * 0.0025) {
    const h = toHex(oklchToRgbInGamut({ ...lch, L: Math.min(1, Math.max(0, l)) }));
    last = h;
    if (ok(h)) return h;
  }
  return last;
}

/** Lift the slots that miss T1 to T7, one slot at a time. A lift moves
 *  away from the ground, with two exceptions. Bright black comes down
 *  when it reads as loud as body text, and when one color of a pair
 *  runs out of room, the other steps toward the ground, never under its
 *  own floor. Half the fit's searches start from here. */
export function tune(src: XtermPalette): XtermPalette {
  const p = { ...src };
  const bg = p.background;
  const gL = lightness(bg);
  const out = isDark(p) ? 1 : -1;
  const set = (slot: GameSlot, hex: string) => {
    if (hex === p[slot]) return false;
    p[slot] = hex;
    return true;
  };
  const dirOf = (hex: string) => (lightness(hex) >= gL ? 1 : -1);
  const floorOf = (slot: GameSlot): ((hex: string) => boolean) => {
    if (slot === 'foreground') return (h) => lc(h, bg) >= T.fgLc && wcag(h, bg) >= T.fgWcag;
    if (SENTENCE.includes(slot)) {
      return (h) => lc(h, bg) >= T.sentenceLc && wcag(h, bg) >= T.sentenceWcag;
    }
    if (CUE.includes(slot)) return (h) => lc(h, bg) >= T.cueLc;
    return () => true;
  };
  // Push `far` away from the ground until `ok` holds. When it runs out
  // of room, pull `near` toward the ground while it keeps its floor.
  const separate = (near: GameSlot, far: GameSlot, ok: (q: XtermPalette) => boolean) => {
    if (ok(p)) return false;
    const nf = stepAtHue(p[far], dirOf(p[far]), (h) => ok({ ...p, [far]: h }));
    if (ok({ ...p, [far]: nf })) return set(far, nf);
    let changed = set(far, nf);
    const nn = stepAtHue(
      p[near],
      -dirOf(p[near]),
      (h) => ok({ ...p, [near]: h }) || !floorOf(near)(h),
    );
    if (floorOf(near)(nn)) changed = set(near, nn) || changed;
    return changed;
  };
  const stepOut = (q: XtermPalette, slot: GameSlot) => Math.abs(lightness(q[slot]) - gL);
  const nearFirst = (a: AnsiSlot, b: AnsiSlot): [AnsiSlot, AnsiSlot] =>
    away(p, p[a]) <= away(p, p[b]) ? [a, b] : [b, a];

  for (let pass = 0; pass < 16; pass += 1) {
    let changed = false;
    changed = set('foreground', stepAtHue(p.foreground, out, floorOf('foreground'))) || changed;
    for (const k of SENTENCE) changed = set(k, stepAtHue(p[k], out, floorOf(k))) || changed;
    for (const k of CUE) changed = set(k, stepAtHue(p[k], out, floorOf(k))) || changed;
    const top = Math.min(T.dimMax, lc(p.foreground, bg) - T.dimBelowFg);
    const dim = lc(p.brightBlack, bg);
    if (dim < T.dimMin) {
      const lifted = stepAtHue(p.brightBlack, out, (h) => lc(h, bg) >= T.dimMin);
      changed = set('brightBlack', lifted) || changed;
    } else if (dim > top + 0.05) {
      const quieted = stepAtHue(p.brightBlack, -out, (h) => lc(h, bg) <= top);
      changed = set('brightBlack', quieted) || changed;
    }
    if (wcag(p.black, bg) < T.blackWcag) {
      const lifted = stepAtHue(p.black, out, (h) => wcag(h, bg) >= T.blackWcag);
      changed = set('black', lifted) || changed;
    }
    for (const [n, b] of PAIRS) {
      changed =
        separate(
          n,
          b,
          (q) => dE(q[n], q[b]) >= T.pairDE && stepOut(q, b) - stepOut(q, n) >= T.pairDL,
        ) || changed;
    }
    changed =
      separate(
        'foreground',
        'brightWhite',
        (q) => stepOut(q, 'brightWhite') - stepOut(q, 'foreground') >= T.fgBrightWhiteDL,
      ) || changed;
    for (const kind of ['protan', 'deutan'] as const) {
      for (const [a, b, need] of CVD_PAIRS) {
        const [near, far] = nearFirst(a, b);
        changed = separate(near, far, (q) => dECvd(q[a], q[b], kind) >= need) || changed;
      }
    }
    {
      const [near, far] = nearFirst('red', 'yellow');
      changed = separate(near, far, (q) => dL(q.red, q.yellow) >= T.rySep) || changed;
    }
    changed =
      separate(
        'red',
        'brightRed',
        (q) => dL(q.red, q.brightRed) >= T.rRbDL && stepOut(q, 'brightRed') > stepOut(q, 'red'),
      ) || changed;
    {
      const [near, far] = nearFirst('cyan', 'green');
      changed =
        separate(near, far, (q) => dECvd(q.cyan, q.green, 'tritan') >= T.tritanCG) || changed;
    }
    if (!changed) break;
  }
  return p;
}

// ── The fit ────────────────────────────────────────────────────────

// How far short of its target each missed check falls, summed. A miss
// counts a little even when it is close, so the search prefers fewer
// misses.
function shortfall(p: XtermPalette): number {
  let s = 0;
  for (const c of checks(p)) {
    if (c.ok) continue;
    if (c.need.startsWith('>=')) {
      const need = +c.need.slice(2);
      s += (need - c.value) / Math.max(1, need) + 0.05;
    } else {
      const [a, b] = c.need.split('..').map(Number);
      s += (c.value < a ? (a - c.value) / a : (c.value - b) / b) + 0.05;
    }
  }
  return s;
}

// How far a check falls outside its target, 0 when it is inside, to
// the tenth checks() reports.
function gap(c: GameCheck): number {
  if (c.need.startsWith('>=')) return Math.max(0, +c.need.slice(2) - c.value);
  const [a, b] = c.need.split('..').map(Number);
  return Math.max(0, a - c.value, c.value - b);
}

// Whether `p` holds every check `held` passes, and falls no further
// short than `held` on each check it misses. A fit for a color vision
// holds to the checks of the Typical fit this way, so no color drops
// under a floor the Typical fit holds, and a color the Typical fit
// leaves short, such as a dim red, gets no fainter. The T7 pairs
// measured through another vision are for players who see through it,
// and a fit for `kind` leaves them out.
function keeps(p: XtermPalette, held: readonly GameCheck[], kind: Cvd): boolean {
  return checks(p).every((c, i) => {
    if (seenThroughOther(c.id, kind)) return true;
    return held[i].ok ? c.ok : gap(c) <= gap(held[i]);
  });
}

// Whether check `id` measures a pair through a color vision other than
// `kind`.
function seenThroughOther(id: string, kind: Cvd): boolean {
  const seen = /^T7 (protan|deutan|tritan) /.exec(id);
  return seen !== null && seen[1] !== kind;
}

/** Whether a fit for `vision` holds the check `id` as the Typical fit
 *  has it: every check but a T7 pair measured through another vision. */
export function holdsCheck(id: string, vision: ColorVision): boolean {
  const kind = CVD_OF[vision];
  return !kind || !seenThroughOther(id, kind);
}

// ── What a color vision keeps apart ────────────────────────────────

type CuePair = readonly [AnsiSlot, AnsiSlot];

// Hits on you in red against tells in green and says in bright yellow,
// and HP at 20 percent in red against HP at 40 percent in yellow.
const RED_PAIRS: readonly CuePair[] = [
  ['red', 'green'],
  ['red', 'brightYellow'],
  ['red', 'yellow'],
];
// Yells in cyan against tells in green and against blue.
const CYAN_PAIRS: readonly CuePair[] = [
  ['cyan', 'green'],
  ['cyan', 'blue'],
];
/** The cue pairs a fit for a color vision keeps from running together
 *  as that vision sees them: the pairs any vision keeps apart, and tells
 *  in green against says in bright yellow and against yellow. A pair the
 *  vision keeps apart never comes nearer than the Typical fit has it,
 *  unless it stays past its target. Any other may come nearer, but by no
 *  more than a quarter of where the Typical fit has it (GUARD_SHARE), and
 *  never under VISION_GUARD, or under where the Typical fit has it if
 *  that is less, so tells in green keep clear of yells in cyan. */
export const GUARDED_PAIRS: readonly CuePair[] = [
  ...RED_PAIRS,
  ...CYAN_PAIRS,
  ['green', 'brightYellow'],
  ['green', 'yellow'],
];

/** How near, in OKLab dE times 100 as a color vision sees them, a fit
 *  for that vision lets two cue colors or two status colors it does not
 *  part come, the floor the Themes review asks of most T7 pairs. */
export const VISION_GUARD = 10;

/** The share of where the Typical fit has a guarded cue pair that the
 *  vision does not keep apart (GUARDED_PAIRS) which a fit for that
 *  vision holds it to, as the vision sees it. */
export const GUARD_SHARE = 0.75;

/** The text colors a cue color must never run into: body text, and the
 *  white and bold white the game writes whole sentences in. A fit for a
 *  color vision keeps each color it moves (visionSlots) from coming
 *  nearer each of them, as that vision sees them, than the Typical fit
 *  has it, or VISION_GUARD if that is less, so a tell in green never
 *  reads as body text. */
export const TEXT_SLOTS: readonly GameSlot[] = ['foreground', 'white', 'brightWhite'];

/** How far short of its target, in OKLab dE times 100, a pair may stand
 *  and still count as apart: the least difference OKLab counts as one
 *  an eye tells. */
export const VISION_SLACK = 2;

/** The least change, in OKLab dE times 100, a fit for a color vision
 *  makes to the color it moves most. A fit that moves nothing this far
 *  is a change no one sees, so the fit keeps the Typical colors, and
 *  the Color vision row counts a change only this large. */
export const VISIBLE_CHANGE = 3;

/** The most any slot of `keys` moves from `a` to `b`, in OKLab dE times
 *  100 as a typical eye sees it. A color that is not hex counts as no
 *  move when it stays the same and as a move past any bound when not. */
export function largestMove<K extends string>(
  a: Readonly<Record<K, string>>,
  b: Readonly<Record<K, string>>,
  keys: readonly K[],
): number {
  let most = 0;
  for (const k of keys) {
    if (a[k] === b[k]) continue;
    const p = parseHex(a[k]);
    const q = parseHex(b[k]);
    most = Math.max(most, p && q ? deltaEOk(p, q) : Infinity);
  }
  return most;
}

/** The cue pairs a fit for `vision` keeps apart. Every vision other
 *  than Typical keeps red apart from green, bright yellow and yellow,
 *  and Tritanopia also keeps cyan apart from green and blue. Typical
 *  names none. */
export function visionPairs(vision: ColorVision): readonly CuePair[] {
  const kind = CVD_OF[vision];
  if (!kind) return [];
  return kind === 'tritan' ? [...RED_PAIRS, ...CYAN_PAIRS] : RED_PAIRS;
}

/** One cue pair as a color vision sees it. `value` is how far apart the
 *  vision sees the two colors of a palette, and `need` how far apart a
 *  typical eye sees them in the Typical fit, each in OKLab dE times 100
 *  to one decimal. */
export interface VisionCheck {
  id: string;
  value: number;
  need: number;
  ok: boolean;
}

/** Each pair `vision` keeps apart (visionPairs), measured in `p` through
 *  that vision against what a typical eye sees of it in `typical`, the
 *  Typical fit laid over the published colors. A pair within
 *  VISION_SLACK of its target counts as apart. */
export function visionChecks(
  p: XtermPalette,
  typical: XtermPalette,
  vision: ColorVision,
): VisionCheck[] {
  const kind = CVD_OF[vision];
  if (!kind) return [];
  return visionPairs(vision).map(([a, b]) => {
    const need = dE(typical[a], typical[b]);
    const value = dECvd(p[a], p[b], kind);
    const ok = value >= need - VISION_SLACK;
    return { id: `${a}/${b}`, value: +value.toFixed(1), need: +need.toFixed(1), ok };
  });
}

/** Whether `p` keeps every pair `vision` keeps apart at least as far
 *  apart, seen through it, as a typical eye sees it in `typical`, give
 *  or take VISION_SLACK. A palette that holds them as its Typical fit
 *  stands needs no fit for the vision. Typical holds every palette. */
export function holdsVision(p: XtermPalette, typical: XtermPalette, vision: ColorVision): boolean {
  return visionChecks(p, typical, vision).every((c) => c.ok);
}

/** The slots a fit for `vision` may move, the slots of the pairs it
 *  keeps apart and the bold or plain twin of each, which T6 ties to it.
 *  Every other slot keeps its Typical fit. Typical names none. */
export function visionSlots(vision: ColorVision): readonly GameSlot[] {
  const named = new Set<GameSlot>();
  for (const [a, b] of visionPairs(vision)) {
    for (const k of [a, b]) {
      for (const pair of PAIRS) if (pair.includes(k)) pair.forEach((s) => named.add(s));
    }
  }
  return GAME_SLOTS.filter((k) => named.has(k));
}

/** Each color a fit for `vision` moves (visionSlots) against body text,
 *  white and bold white (TEXT_SLOTS), the pairs it keeps from running
 *  together as that vision sees them. Typical names none. */
export function textGuards(vision: ColorVision): readonly (readonly [GameSlot, GameSlot])[] {
  return visionSlots(vision).flatMap((k) => TEXT_SLOTS.map((t) => [k, t] as const));
}

/** The slots a fit for `vision` may turn in hue, each up OKLCH hue
 *  (turnHue): red toward orange and green toward teal for every vision,
 *  and cyan toward blue for a tritanope. Their bold twins keep their
 *  hue. Typical names none. */
export function turnSlots(vision: ColorVision): readonly TurnFamily[] {
  const kind = CVD_OF[vision];
  if (!kind) return [];
  return kind === 'tritan' ? ['red', 'green', 'cyan'] : ['red', 'green'];
}

// ── The searches ───────────────────────────────────────────────────

// The Typical search's steps and seeds, and what a step of distance
// costs every search.
const ITERS = 9000;
const SEEDS = [11, 23, 37];
const MOVE_WEIGHT = 0.35;
// What each degree of a turned hue costs a fit for a color vision, so
// it turns only as far as parting a pair asks.
const TURN_WEIGHT = 0.1;
// A turn past HUE_TURN must close at least this much of the pairs' gap,
// in OKLab dE times 100, or the fit keeps the smaller turn.
const FAR_GAIN = 0.5;

type Lightness = Record<GameSlot, number>;
type Turns = Partial<Record<GameSlot, number>>;

interface Search {
  cost: number;
  palette: XtermPalette;
}

// A linear congruential generator in plain doubles. The products pass
// 2^53 and lose low bits, and the fit depends on exactly those values,
// so integer math would draw a different sequence.
function lcg(seed: number): () => number {
  let s = seed;
  return () => {
    s = (s * 1103515245 + 12345) % 2147483648;
    return s / 2147483648;
  };
}

const oklchOf = (p: XtermPalette) =>
  Object.fromEntries(GAME_SLOTS.map((k) => [k, rgbToOklch(rgb(p[k]))])) as Record<GameSlot, Oklch>;

// One random search over the slots' lightness, from `start` or from
// the published colors. It keeps a step only when the cost drops. The
// cost is the shortfall first, then the total distance from the
// published colors, and a heavy charge on any slot that loses more
// than 40 percent of its chroma, so a yellow never turns cream.
function search(src: XtermPalette, seed: number, start: XtermPalette | undefined): Search {
  const rnd = lcg(seed);
  const base = oklchOf(src);
  const at = (k: GameSlot, L: number) => {
    const o = base[k];
    if (Math.abs(L - o.L) < 1e-9) return { hex: src[k], C: o.C };
    const hex = toHex(oklchToRgbInGamut({ ...o, L: Math.max(0, Math.min(1, L)) }));
    return { hex, C: rgbToOklch(rgb(hex)).C };
  };
  const cost = (state: Lightness): Search => {
    const p = { ...src };
    let drained = 0;
    for (const k of GAME_SLOTS) {
      const { hex, C } = at(k, state[k]);
      p[k] = hex;
      const c0 = base[k].C;
      if (c0 > 0.04 && C < 0.6 * c0) drained += (0.6 * c0 - C) / c0;
    }
    let move = 0;
    for (const k of GAME_SLOTS) move += dE(src[k], p[k]);
    const missed = shortfall(p) * 100;
    return { cost: missed + (move * MOVE_WEIGHT) / 10 + drained * 400, palette: p };
  };
  let cur = Object.fromEntries(
    GAME_SLOTS.map((k) => [k, start ? rgbToOklch(rgb(start[k])).L : base[k].L]),
  ) as Lightness;
  let best = cost(cur);
  for (let i = 0; i < ITERS; i += 1) {
    const t = { ...cur };
    const amp = 0.06 * (1 - i / ITERS) + 0.004;
    const n = 1 + Math.floor(rnd() * 3);
    for (let j = 0; j < n; j += 1) {
      const k = GAME_SLOTS[Math.floor(rnd() * GAME_SLOTS.length)];
      // A quarter of the moves pull a slot back toward its published
      // lightness.
      const L0 = base[k].L;
      t[k] = rnd() < 0.25 ? L0 + (t[k] - L0) * rnd() : t[k] + (rnd() - 0.5) * 2 * amp;
      t[k] = Math.max(0, Math.min(1, t[k]));
    }
    const c = cost(t);
    if (c.cost < best.cost) {
      best = c;
      cur = t;
    }
  }
  return best;
}

// What a search for a color vision holds to.
interface Hold {
  kind: Cvd;
  // The Typical fit laid over the published colors. The search starts
  // here and measures each move from here.
  from: XtermPalette;
  // checks() of `from`, the floors no step may give up.
  held: readonly GameCheck[];
  // The slots the search may move (visionSlots), and the ones it may
  // turn (turnSlots).
  slots: readonly GameSlot[];
  turns: readonly TurnFamily[];
  // The pairs the vision keeps apart, each with its target, how far
  // apart a typical eye sees it in `from`.
  pairs: readonly (readonly [AnsiSlot, AnsiSlot, number])[];
  // The cue pairs, and each color the search moves against body text,
  // white and bold white, each with the distance, as the vision sees it,
  // it may not drop under (GUARDED_PAIRS, TEXT_SLOTS).
  guards: readonly (readonly [GameSlot, GameSlot, number])[];
}

interface VisionState {
  L: Lightness;
  turn: Turns;
}

interface VisionRun extends Search {
  state: VisionState;
  // How far the pairs fall short of their targets, summed, in OKLab dE
  // times 100.
  gap: number;
}

// The pairs' shortfall the way shortfall() sums the checks', and their
// gap in dE.
function pairShort(p: XtermPalette, hold: Hold): { short: number; gap: number } {
  let short = 0;
  let missed = 0;
  for (const [a, b, need] of hold.pairs) {
    const v = dECvd(p[a], p[b], hold.kind);
    if (v >= need) continue;
    short += (need - v) / need + 0.05;
    missed += need - v;
  }
  return { short, gap: missed };
}

// The grids a vision search sweeps, coarse to fine: the step in OKLCH
// lightness, how many steps it reaches each way, and the step in
// degrees of hue a turn takes.
const SWEEPS: readonly (readonly [number, number, number])[] = [
  [0.02, 15, 2],
  [0.01, 4, 1],
  [0.005, 4, 1],
  [0.0025, 4, 1],
];
// How many times a sweep goes over the slots before it gives up.
const PASSES = 6;

// A search for a color vision over the lightness of the slots `hold`
// names, and the hue of the ones it may turn, each turn up to `bound`
// degrees. It starts from `start` and sweeps one grid at a time, coarse
// to fine: each slot over its lightness and its turn together, each
// slot with its twin over both their lightness, and two slots with
// their twins by one step together, keeping each step that lowers the
// cost. It takes no step that gives up a check the Typical
// fit passes or falls further short of one it misses, or that brings a
// guarded pair too near. The cost is how far the pairs fall short of
// their targets, then the total distance from the Typical fit, the
// degrees turned, and the same charge on chroma as the Typical search.
function visionSearch(src: XtermPalette, hold: Hold, start: VisionState, bound: number): VisionRun {
  const base = oklchOf(src);
  const home = oklchOf(hold.from);
  const at = (k: GameSlot, L: number, turn: number) => {
    const o = base[k];
    if (turn === 0 && Math.abs(L - o.L) < 1e-9) return { hex: src[k], C: o.C };
    if (turn === 0 && Math.abs(L - home[k].L) < 1e-9) return { hex: hold.from[k], C: home[k].C };
    const h = (o.h + turn) % 360;
    const hex = toHex(oklchToRgbInGamut({ L: Math.max(0, Math.min(1, L)), C: o.C, h }));
    return { hex, C: rgbToOklch(rgb(hex)).C };
  };
  const cost = (state: VisionState): VisionRun => {
    const p = { ...src };
    let drained = 0;
    let turned = 0;
    for (const k of GAME_SLOTS) {
      const turn = state.turn[k] ?? 0;
      const { hex, C } = at(k, state.L[k], turn);
      p[k] = hex;
      turned += turn;
      const c0 = base[k].C;
      if (c0 > 0.04 && C < 0.6 * c0) drained += (0.6 * c0 - C) / c0;
    }
    const closer = hold.guards.some(([a, b, floor]) => dECvd(p[a], p[b], hold.kind) < floor);
    if (closer || !keeps(p, hold.held, hold.kind)) {
      return { cost: Infinity, gap: Infinity, palette: p, state };
    }
    let move = 0;
    for (const k of GAME_SLOTS) move += dE(hold.from[k], p[k]);
    const { short, gap: missed } = pairShort(p, hold);
    const total = short * 100 + (move * MOVE_WEIGHT) / 10 + turned * TURN_WEIGHT + drained * 400;
    return { cost: total, gap: missed, palette: p, state };
  };
  const { slots, turns } = hold;
  // How far each slot may turn, under the bound and its family's limit.
  const room: Turns = {};
  for (const k of turns) room[k] = turnRoom(k, base[k].h, bound);
  // Each pair of slots, each with its twin where the search may move it.
  const family = (k: GameSlot) => {
    const twin = TWIN[k];
    return twin && slots.includes(twin) ? [k, twin] : [k];
  };
  const groups: GameSlot[][] = [];
  slots.forEach((a, i) => {
    for (const b of slots.slice(i + 1)) {
      const group = [...new Set([...family(a), ...family(b)])];
      if (!groups.some((g) => g.length === group.length && g.every((k) => group.includes(k)))) {
        groups.push(group);
      }
    }
  });
  let best = cost(start);
  const consider = (state: VisionState) => {
    const c = cost(state);
    if (c.cost >= best.cost) return false;
    best = c;
    return true;
  };
  const clamp = (L: number) => Math.max(0, Math.min(1, L));
  for (const [step, reach, turnStep] of SWEEPS) {
    for (let pass = 0; pass < PASSES; pass += 1) {
      let moved = false;
      for (const k of slots) {
        // The slot over its lightness and its turn.
        const now = best.state;
        const t0 = now.turn[k] ?? 0;
        const most = room[k] ?? 0;
        const turnsTried: number[] = [t0];
        if (most > 0) {
          const span = step === SWEEPS[0][0] ? most : 2 * turnStep;
          for (let t = Math.max(0, t0 - span); t <= Math.min(most, t0 + span); t += turnStep) {
            if (t !== t0) turnsTried.push(t);
          }
        }
        for (let i = -reach; i <= reach; i += 1) {
          for (const t of turnsTried) {
            if (i === 0 && t === t0) continue;
            const L = { ...now.L, [k]: clamp(now.L[k] + i * step) };
            moved =
              consider({ L, turn: t === 0 ? omit(now.turn, k) : { ...now.turn, [k]: t } }) || moved;
          }
        }
        // The slot and its twin over both their lightness.
        const twin = TWIN[k];
        if (!twin || !slots.includes(twin) || GAME_SLOTS.indexOf(twin) < GAME_SLOTS.indexOf(k)) {
          continue;
        }
        const pair = best.state;
        for (let i = -reach; i <= reach; i += 1) {
          for (let j = -reach; j <= reach; j += 1) {
            if (i === 0 || j === 0) continue;
            const L = {
              ...pair.L,
              [k]: clamp(pair.L[k] + i * step),
              [twin]: clamp(pair.L[twin] + j * step),
            };
            moved = consider({ L, turn: pair.turn }) || moved;
          }
        }
      }
      // Two slots and their twins together, as one step, so colors that
      // floors tie to each other move as one where each alone would break
      // one, such as green with yellow and bright yellow for a protanope.
      for (const group of groups) {
        const now = best.state;
        for (let i = -reach; i <= reach; i += 1) {
          if (i === 0) continue;
          const L = { ...now.L };
          for (const k of group) L[k] = clamp(now.L[k] + i * step);
          moved = consider({ L, turn: now.turn }) || moved;
        }
      }
      if (!moved) break;
    }
  }
  // Last, try each turned hue at every whole degree up to its turn, and
  // keep the smallest turn that costs no more.
  for (const k of turns) {
    const now = best.state.turn[k] ?? 0;
    for (let t = 0; t < now; t += 1) {
      const turn = t === 0 ? omit(best.state.turn, k) : { ...best.state.turn, [k]: t };
      const c = cost({ L: best.state.L, turn });
      if (c.cost <= best.cost) {
        best = c;
        break;
      }
    }
  }
  return best;
}

// `turns` without the turn of `k`.
function omit(turns: Turns, k: GameSlot): Turns {
  const out = { ...turns };
  delete out[k];
  return out;
}

/** The colors fit() reads, the ground, body text and the 16 ANSI
 *  colors, as one string. Two palettes with the same key fit the same. */
export function fitKey(p: XtermPalette): string {
  return [p.background, ...GAME_SLOTS.map((k) => p[k])].join(' ');
}

/** Whether a fit of `p` can move anything. A palette that passes every
 *  check fits to no change, which a config cannot keep, since it leaves
 *  an empty fit out, so Vosh never fits one. Neither does it fit a
 *  palette with a color that is not hex, which the fit cannot read. */
export function needsFit(p: XtermPalette): boolean {
  try {
    return checks(p).some((c) => !c.ok);
  } catch {
    return false;
  }
}

/** Whether a fit of `p` for `vision` can move anything past `typical`,
 *  its Typical fit: false where the Typical fit already keeps every
 *  pair the vision keeps apart, and for a palette with a color that is
 *  not hex. */
export function needsVisionFit(
  p: XtermPalette,
  vision: ColorVision,
  typical: Partial<XtermPalette> = {},
): boolean {
  try {
    const from = { ...p, ...typical };
    return !holdsVision(from, from, vision);
  } catch {
    return false;
  }
}

// The slots of `p` that differ from `src`.
function movedFrom(src: XtermPalette, p: XtermPalette): Partial<XtermPalette> {
  const moved: Partial<XtermPalette> = {};
  for (const k of GAME_SLOTS) if (p[k] !== src[k]) moved[k] = p[k];
  return moved;
}

// The first of the cheapest wins.
const cheapest = <S extends Search>(runs: S[]) =>
  runs.reduce((win, run) => (run.cost < win.cost ? run : win));

/** Fit a published palette to the checks, for `vision`, and return only
 *  the slots it moved. What still misses stays missed, so checks() on
 *  the fitted palette says what is short, and visionChecks() what a
 *  vision still sees too close.
 *
 *  Typical is the best of six searches, three seeds each from the
 *  published colors and from tune(), exactly as before color vision. It
 *  takes about two seconds, so a built in theme stores its result.
 *
 *  Another vision builds on the Typical fit, `typical` when you have it.
 *  It keeps that fit when the fit already keeps every pair the vision
 *  keeps apart (holdsVision). Else it moves only the slots of those
 *  pairs and their twins (visionSlots), so every other color plays as
 *  Typical has it. No step gives up a check the Typical fit passes or
 *  falls further short of one it misses (holdsCheck), brings a cue pair
 *  under its guard (GUARDED_PAIRS), or brings a color it moves under its
 *  guard from body text, white or bold white (textGuards). It moves
 *  lightness first, in one search from the Typical fit (visionSearch).
 *  Where that leaves a pair short, a search from there may also turn
 *  red, green and for a tritanope cyan up to HUE_TURN degrees, kept
 *  where it parts the pairs further, and where that still leaves one
 *  short, one more up to HUE_TURN_FAR, kept where it closes FAR_GAIN
 *  more of the gap. Where the best of them moves no color as far as
 *  VISIBLE_CHANGE, the fit is the Typical fit. That takes up to two
 *  seconds. */
export function fit(
  src: XtermPalette,
  vision: ColorVision = 'typical',
  typical?: Partial<XtermPalette>,
): Partial<XtermPalette> {
  const kind = CVD_OF[vision];
  if (!kind) {
    const runs = [undefined, tune(src)].flatMap((start) =>
      SEEDS.map((seed) => search(src, seed, start)),
    );
    return movedFrom(src, cheapest(runs).palette);
  }
  const base = typical ?? fit(src);
  const from = { ...src, ...base };
  if (holdsVision(from, from, vision)) return base;
  const targets = visionPairs(vision);
  const hold: Hold = {
    kind,
    from,
    held: checks(from),
    slots: visionSlots(vision),
    turns: turnSlots(vision),
    pairs: targets.map(([a, b]) => [a, b, dE(from[a], from[b])] as const),
    guards: [
      ...GUARDED_PAIRS.map(([a, b], i) => {
        const seen = dECvd(from[a], from[b], kind);
        const floor = targets.includes(GUARDED_PAIRS[i])
          ? dE(from[a], from[b])
          : Math.max(VISION_GUARD, GUARD_SHARE * seen);
        return [a, b, Math.min(seen, floor)] as const;
      }),
      ...textGuards(vision).map(
        ([a, b]) => [a, b, Math.min(dECvd(from[a], from[b], kind), VISION_GUARD)] as const,
      ),
    ],
  };
  const home: VisionState = {
    L: Object.fromEntries(GAME_SLOTS.map((k) => [k, rgbToOklch(rgb(from[k])).L])) as Lightness,
    turn: {},
  };
  const runs = (start: VisionState, bound: number) => visionSearch(src, hold, start, bound);
  let best = runs(home, 0);
  if (best.gap > 0) {
    const near = runs(best.state, HUE_TURN);
    if (near.gap < best.gap) best = near;
  }
  if (best.gap > 0) {
    const far = runs(best.state, HUE_TURN_FAR);
    if (far.gap < best.gap - FAR_GAIN) best = far;
  }
  const p = best.palette;
  // Where every step that parts a pair further gives something up, or
  // the best moves no color far enough to see, the Typical fit plays.
  if (largestMove(from, p, GAME_SLOTS) < VISIBLE_CHANGE) return base;
  return movedFrom(src, p);
}
