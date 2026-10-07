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
// config (settings/appearance/fitAndKeep). The main window fits a
// custom theme in play that keeps no fit when it loads your config, and
// holds that fit in memory only (customThemeFits, holdFit in themes.ts).
// Every fit runs in a worker (fitOffThread, gameFit.worker). Play draws
// the fit while Fit game colors is on (playPalette in themes.ts,
// fitGameColors).
//
// A color vision other than Typical swaps the colors that vision runs
// together for ones it tells apart, the way color blind modes in games
// do (swapFor). For deuteranopia and protanopia green turns blue, red
// turns toward vermilion and blue turns violet. For tritanopia blue turns
// purple and magenta turns pink (SWAP_TARGETS). The swap starts from the
// palette Typical plays, the Typical fit or the published colors, and
// then settles the lightness of every cue color and the hue of each
// turned one inside its window, by floors in strict order. No two of the
// game's channels come nearer than CHANNEL_LEAST, so where the theme's
// own text and channels already fill the blues, a color keeps its hue
// rather than run into one. A built in theme ships its swaps in
// themes.ts (VISION_FITS), and gameFit.test.ts works them out again with
// VOSH_FIT_THEMES=1.

import { ANSI_SLOTS, type AnsiSlot } from './baseAnsi';
import {
  contrast,
  deltaEOk,
  indexedRgb,
  linearToOklab,
  linearToRgb,
  luminance,
  oklchToRgbInGamut,
  parseHex,
  rgbToLinear,
  rgbToOklab,
  rgbToOklch,
  toHex,
  type Oklab,
  type Oklch,
  type Rgb,
} from './color';
import { GAME_CHANNEL_SLOTS } from './gameChannels';
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

const apcaYOf = (c: Rgb) =>
  0.2126729 * (c.r / 255) ** 2.4 + 0.7151522 * (c.g / 255) ** 2.4 + 0.072175 * (c.b / 255) ** 2.4;

const apcaY = (hex: string) => apcaYOf(rgb(hex));

/** APCA lightness contrast of `text` on `ground`, positive for dark
 *  text on light, negative for light text on dark. */
export function apca(text: string, ground: string): number {
  return apcaOfY(apcaY(text), apcaY(ground));
}

// APCA from the screen luminance of the text and of the ground.
function apcaOfY(textY: number, groundY: number): number {
  const clampY = (y: number) => (y < 0.022 ? y + (0.022 - y) ** 1.414 : y);
  const yt = clampY(textY);
  const yb = clampY(groundY);
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

/** The color vision Vosh swaps the cue colors for, from UiConfig
 *  color_vision. Typical asks the 46 checks as they stand, and every
 *  fit for Typical is the one before color vision. Each other vision
 *  swaps the colors it runs together for ones it tells apart, in the
 *  game text (swapFor) and in the window's status colors (chrome
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

// ── The fixed colors the game sends ────────────────────────────────

/** The color xterm draws for 256 color index `n`, 16 to 255. */
export function xterm256(n: number): string {
  return toHex(indexedRgb(n, []));
}

/** The 256 color indexes the game sends whatever the theme. 240 is the
 *  Wizi and Incog prompt prefix, the rest are minimap glyphs, and 213
 *  is your own @ on the minimap. */
export const GAME_FIXED_COLORS = [
  240, 249, 180, 77, 34, 143, 241, 75, 33, 58, 117, 220, 196, 255, 213,
] as const;

// ── The targets ────────────────────────────────────────────────────

/** How far apart in OKLab dE times 100 two game colors stand at least
 *  for the fit to call them two colors. Customize vitals marks a color
 *  this near the low or warn tone. */
export const PAIR_DE = 10;

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
  pairDE: PAIR_DE,
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

// ── What the checks read of a color ────────────────────────────────

/** What the checks read of one color, worked out once: its OKLab
 *  coordinates, its APCA and WCAG luminance, and its OKLab as each
 *  deficiency sees it, worked out when first asked. */
interface Tone {
  hex: string;
  rgb: Rgb;
  lab: Oklab;
  y: number;
  lum: number;
  seen: Partial<Record<Cvd, Oklab>>;
}

function toneOf(hex: string): Tone {
  const c = rgb(hex);
  return { hex, rgb: c, lab: rgbToOklab(c), y: apcaYOf(c), lum: luminance(c), seen: {} };
}

const seenTone = (t: Tone, kind: Cvd): Oklab =>
  (t.seen[kind] ??= linearToOklab(simulateRgb(t.rgb, kind)));

// OKLab dE times 100 between two sets of coordinates.
const labDE = (p: Oklab, q: Oklab) => 100 * Math.hypot(p.L - q.L, p.a - q.a, p.b - q.b);

// The WCAG contrast of two relative luminances.
const ratio = (la: number, lb: number) => (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);

type ToneOf = (slot: 'background' | GameSlot) => Tone;

/** The id and target of every check, in the order the review lists
 *  them. T4 takes its range from body text (measure). */
const CHECK_SPEC: readonly (readonly [string, string])[] = [
  ['T1 fg Lc', '>=75'],
  ['T1 fg WCAG', '>=7'],
  ...SENTENCE.map((k) => [`T2 ${k} Lc`, '>=60'] as const),
  ...CUE.map((k) => [`T3 ${k} Lc`, '>=45'] as const),
  ['T4 brightBlack Lc', ''],
  ['T5 black WCAG', '>=1.25'],
  ...PAIRS.flatMap(([n]) => [
    [`T6 ${n} pair dE`, '>=10'] as const,
    [`T6 ${n} bright step dL`, '>=4'] as const,
  ]),
  ['T6 fg/brightWhite dL', '>=8'],
  ...(['protan', 'deutan'] as const).flatMap((kind) =>
    CVD_PAIRS.map(([a, b, need]) => [`T7 ${kind} ${a}/${b}`, `>=${need}`] as const),
  ),
  ['T7 red/yellow dL', '>=10'],
  ['T7 red/brightRed dL', '>=8'],
  ['T7 tritan cyan/green', '>=8'],
];
const T4_AT = CHECK_SPEC.findIndex(([id]) => id === 'T4 brightBlack Lc');

interface Measured {
  values: number[];
  oks: boolean[];
  // The top of the T4 range.
  dimTop: number;
}

// Every check of the colors `t` gives, in CHECK_SPEC order, each value
// unrounded and whether it passes.
function measure(t: ToneOf): Measured {
  const bg = t('background');
  const values: number[] = [];
  const oks: boolean[] = [];
  const add = (value: number, ok: boolean) => {
    values.push(value);
    oks.push(ok);
  };
  const lcOf = (k: GameSlot) => Math.abs(apcaOfY(t(k).y, bg.y));
  const wcagOf = (k: GameSlot) => ratio(t(k).lum, bg.lum);
  const awayOf = (k: GameSlot) => Math.abs(t(k).lab.L * 100 - bg.lab.L * 100);
  const dLOf = (a: GameSlot, b: GameSlot) => Math.abs(t(a).lab.L * 100 - t(b).lab.L * 100);
  const cvdOf = (a: GameSlot, b: GameSlot, kind: Cvd) =>
    labDE(seenTone(t(a), kind), seenTone(t(b), kind));
  const fgLc = lcOf('foreground');
  add(fgLc, fgLc >= T.fgLc);
  add(wcagOf('foreground'), wcagOf('foreground') >= T.fgWcag);
  for (const k of SENTENCE) add(lcOf(k), lcOf(k) >= T.sentenceLc);
  for (const k of CUE) add(lcOf(k), lcOf(k) >= T.cueLc);
  const dim = lcOf('brightBlack');
  const dimTop = Math.min(T.dimMax, fgLc - T.dimBelowFg);
  add(dim, dim >= T.dimMin && dim <= dimTop + 0.05);
  add(wcagOf('black'), wcagOf('black') >= T.blackWcag - 1e-9);
  for (const [n, b] of PAIRS) {
    const pair = labDE(t(n).lab, t(b).lab);
    add(pair, pair >= T.pairDE);
    const step = awayOf(b) - awayOf(n);
    add(step, step >= T.pairDL);
  }
  const fw = awayOf('brightWhite') - awayOf('foreground');
  add(fw, fw >= T.fgBrightWhiteDL);
  for (const kind of ['protan', 'deutan'] as const) {
    for (const [a, b, need] of CVD_PAIRS) {
      const v = cvdOf(a, b, kind);
      add(v, v >= need);
    }
  }
  add(dLOf('red', 'yellow'), dLOf('red', 'yellow') >= T.rySep);
  add(dLOf('red', 'brightRed'), dLOf('red', 'brightRed') >= T.rRbDL);
  const tc = cvdOf('cyan', 'green', 'tritan');
  add(tc, tc >= T.tritanCG);
  return { values, oks, dimTop };
}

/** Every check, in the order the review lists them. */
export function checks(p: XtermPalette): GameCheck[] {
  const tones = new Map<string, Tone>();
  const { values, oks, dimTop } = measure((k) => {
    let tone = tones.get(p[k]);
    if (!tone) {
      tone = toneOf(p[k]);
      tones.set(p[k], tone);
    }
    return tone;
  });
  return CHECK_SPEC.map(([id, need], i) => ({
    id,
    value: +values[i].toFixed(1),
    need: i === T4_AT ? `45..${dimTop.toFixed(0)}` : need,
    ok: oks[i],
  }));
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

// Whether check `id` measures a pair through a color vision other than
// `kind`.
function seenThroughOther(id: string, kind: Cvd): boolean {
  const seen = /^T7 (protan|deutan|tritan) /.exec(id);
  return seen !== null && seen[1] !== kind;
}

/** Whether a swap for `vision` holds the check `id` as its start has
 *  it: every check but a T7 pair measured through another vision, which
 *  is for the players who see through that one. */
export function holdsCheck(id: string, vision: ColorVision): boolean {
  const kind = CVD_OF[vision];
  return !kind || !seenThroughOther(id, kind);
}

// ── The swap for a color vision ────────────────────────────────────

/** Two cue colors. */
export type CuePair = readonly [AnsiSlot, AnsiSlot];

/** A family of cue colors, a plain color and its bold twin. */
export type CueFamily = 'red' | 'green' | 'yellow' | 'blue' | 'magenta' | 'cyan';

/** The twelve colors a swap may move, the six cue families plain and
 *  bold. Body text, white, bold white, black and bold black never move. */
export const CUE_SLOTS: readonly AnsiSlot[] = [
  'red',
  'green',
  'yellow',
  'blue',
  'magenta',
  'cyan',
  'brightRed',
  'brightGreen',
  'brightYellow',
  'brightBlue',
  'brightMagenta',
  'brightCyan',
];

/** The family a cue color belongs to. */
export function familyOf(slot: AnsiSlot): CueFamily {
  return (slot.startsWith('bright') ? slot.slice(6).toLowerCase() : slot) as CueFamily;
}

/** Where a swap turns a family: the OKLCH hue it aims for, how far
 *  either side of it the solve may settle, in degrees, and the least
 *  OKLCH chroma it gives the family where sRGB holds it. */
export interface SwapTarget {
  hue: number;
  reach: number;
  chroma: number;
}

// Tells in green leave the yellow side for the blue pole, hits in red
// move toward vermilion, which a protanope sees brighter, and blue turns
// violet so blue text never reads as a tell. Red stops short of orange,
// so HP at 20 percent stays apart from HP at 40 percent in yellow.
const RED_GREEN_SWAP: Readonly<Partial<Record<CueFamily, SwapTarget>>> = {
  green: { hue: 240, reach: 15, chroma: 0.11 },
  red: { hue: 45, reach: 15, chroma: 0.13 },
  blue: { hue: 290, reach: 10, chroma: 0.11 },
};

/** The families each vision turns, both twins of each, the swaps color
 *  blind palettes and games use. For deuteranopia and protanopia green
 *  turns blue, red toward vermilion and blue violet. For tritanopia blue
 *  turns purple, apart from yells in cyan and tells in green, and magenta
 *  pink to leave room for it. Every other family keeps its hue and
 *  chroma, and only its lightness may move: yellow and cyan under every
 *  vision, magenta under deuteranopia and protanopia, and green under
 *  tritanopia, since turning them runs bold cyan, bold magenta and tells
 *  into body text. A turned color keeps its own hue too where no hue in
 *  its window holds the firm floors (swapFor). Typical turns none. */
export const SWAP_TARGETS: Readonly<
  Record<ColorVision, Readonly<Partial<Record<CueFamily, SwapTarget>>>>
> = {
  typical: {},
  deuteranopia: RED_GREEN_SWAP,
  protanopia: RED_GREEN_SWAP,
  tritanopia: {
    blue: { hue: 320, reach: 10, chroma: 0.12 },
    magenta: { hue: 355, reach: 10, chroma: 0.1 },
  },
};

/** The color that carries the change you see, and its bold twin: green
 *  for deuteranopia and protanopia, blue for tritanopia. */
export const LEAD_SLOTS: Readonly<Record<ColorVision, readonly AnsiSlot[]>> = {
  typical: [],
  deuteranopia: ['green', 'brightGreen'],
  protanopia: ['green', 'brightGreen'],
  tritanopia: ['blue', 'brightBlue'],
};

/** How far the lead color and its bold twin move at least, in OKLab dE
 *  times 100 as a typical eye sees them, so the swap shows. Where the
 *  lead cannot, such as Rose Pine's pine green, which is a blue already,
 *  another plain color the vision turns moves MOVE_MIN.lead. */
export const MOVE_MIN = { lead: 12, bold: 6 } as const;

/** How far the lead color moves where every firmer floor leaves room. */
export const MOVE_WANT = 15;

/** The pairs a vision runs together that the swap exists to part. Each
 *  stands, as the vision sees it, at least PART_MIN apart, or as far as a
 *  typical eye sees it at the start if that is less, give or take
 *  VISION_SLACK. Hits on you in red against tells in green, bold red
 *  against bold green, and tells against faction in yellow and says in
 *  bold yellow. For tritanopia yells in cyan and tells in green against
 *  blue, and clan in bold cyan against cabal in bold blue. */
export const PARTED_PAIRS: Readonly<Record<ColorVision, readonly CuePair[]>> = {
  typical: [],
  deuteranopia: [
    ['red', 'green'],
    ['brightRed', 'brightGreen'],
    ['green', 'yellow'],
    ['green', 'brightYellow'],
  ],
  protanopia: [
    ['red', 'green'],
    ['brightRed', 'brightGreen'],
    ['green', 'yellow'],
    ['green', 'brightYellow'],
  ],
  tritanopia: [
    ['cyan', 'blue'],
    ['green', 'blue'],
    ['brightCyan', 'brightBlue'],
  ],
};

/** How far apart a parted pair stands at least, per vision. Purple sits
 *  nearer the middle of a tritanope's axis than cyan does, so tritanopia
 *  asks less. */
export const PART_MIN: Readonly<Record<ColorVision, number>> = {
  typical: 0,
  deuteranopia: 20,
  protanopia: 20,
  tritanopia: 15,
};

/** The pairs a vision runs together that the swap cannot part further
 *  without breaking a firmer floor. Each comes no nearer, as the vision
 *  sees it, than at the start, give or take KEPT_SLACK: HP at 20 and 40
 *  percent in red and yellow, hits on you against says in bold yellow,
 *  blue against magenta, and for tritanopia yells against tells and red
 *  against green. */
export const KEPT_PAIRS: Readonly<Record<ColorVision, readonly CuePair[]>> = {
  typical: [],
  deuteranopia: [
    ['red', 'yellow'],
    ['red', 'brightYellow'],
    ['blue', 'magenta'],
  ],
  protanopia: [
    ['red', 'yellow'],
    ['red', 'brightYellow'],
    ['blue', 'magenta'],
  ],
  tritanopia: [
    ['cyan', 'green'],
    ['red', 'yellow'],
    ['red', 'brightYellow'],
    ['red', 'green'],
  ],
};

/** How much nearer than at the start a kept pair may come, in OKLab dE
 *  times 100. */
export const KEPT_SLACK = 0.5;

/** The cue colors the game's channels print in, from the table the chat
 *  pane reads (gameChannels): tells in green, faction in yellow, yells
 *  in cyan, immortal talk in bold red, newbie chat in bold green, says in
 *  bold yellow, cabal in bold blue, group tells in bold magenta and clan
 *  in bold cyan. Hits on you in red join them, the color the game prints
 *  its fight lines in. */
const CHANNEL_SET = new Set<AnsiSlot>(GAME_CHANNEL_SLOTS.values());
export const CHANNEL_SLOTS: readonly AnsiSlot[] = CUE_SLOTS.filter(
  (k) => k === 'red' || CHANNEL_SET.has(k),
);

/** The pairs a swap keeps apart as channels: every two channel colors,
 *  and tells and yells against blue text. A swap keeps each pair at
 *  least CHANNEL_FLOOR apart, and never under CHANNEL_LEAST, or as far as
 *  at the start if that is less, both as the vision sees them and as a
 *  typical eye does. */
export const CHANNEL_PAIRS: readonly CuePair[] = [
  ...CHANNEL_SLOTS.flatMap((a, i) => CHANNEL_SLOTS.slice(i + 1).map((b) => [a, b] as const)),
  ['green', 'blue'],
  ['cyan', 'blue'],
];

const pairKey = ([a, b]: CuePair) => [a, b].sort().join('/');
const CHANNEL_KEYS = new Set(CHANNEL_PAIRS.map(pairKey));

/** Every other two cue colors of the same weight, which a swap keeps at
 *  least CUE_FLOOR apart, or as far as at the start if that is less. */
export const CUE_PAIRS: readonly CuePair[] = [CUE_SLOTS.slice(0, 6), CUE_SLOTS.slice(6)].flatMap(
  (row) =>
    row.flatMap((a, i) =>
      row
        .slice(i + 1)
        .map((b) => [a, b] as const)
        .filter((pair) => !CHANNEL_KEYS.has(pairKey(pair))),
    ),
);

/** The floors of channel pairs and of other cue pairs, in OKLab dE
 *  times 100. */
export const CHANNEL_FLOOR = 6;
export const CUE_FLOOR = 3;

/** How near two channels come at the very least, as firm as the checks,
 *  so tells never read as cabal or group tells: twice VISION_SLACK, or as
 *  near as they stood at the start if that is less. Where no hue in its
 *  window keeps a turned color this far from every channel, it keeps its
 *  own hue. */
export const CHANNEL_LEAST = 4;

/** How near, in OKLab dE times 100, a swap lets a color it moves come to
 *  body text, white and bold white, give or take VISION_SLACK, unless
 *  they stood nearer at the start. The window keeps each status color as
 *  far from the text beside it. */
export const VISION_GUARD = 10;

/** The text colors a cue color must never run into: body text, and the
 *  white and bold white the game writes whole sentences in. */
export const TEXT_SLOTS: readonly GameSlot[] = ['foreground', 'white', 'brightWhite'];

/** How far short of its target, in OKLab dE times 100, a pair may stand
 *  and still count as apart: the least difference OKLab counts as one
 *  an eye tells. */
export const VISION_SLACK = 2;

/** How much further short of its target a check the start misses may
 *  fall under a swap. */
export const MISS_SLACK = 1;

/** How far a swap may move a color in OKLCH lightness from the start. */
export const L_REACH = 0.15;

/** The share of its chroma a turned color keeps at least: of its own at
 *  the start, or of its target chroma if that is less. Lightness alone
 *  could clear every distance by lifting a color to white, which shows
 *  no color at all. */
export const CHROMA_KEEP = 0.75;

/** The share of its start chroma a color that keeps its hue keeps at
 *  least, as the Typical fit asks, so a lilac or a cream never lifts to
 *  white. A color under 0.04 of chroma, a gray, keeps none. */
export const HUE_CHROMA_KEEP = 0.6;

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

// ── The Typical search ─────────────────────────────────────────────

// The search's steps and seeds, and what a step of distance costs.
const ITERS = 9000;
const SEEDS = [11, 23, 37];
const MOVE_WEIGHT = 0.35;

type Lightness = Record<GameSlot, number>;

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

// ── The swap search ────────────────────────────────────────────────

// The slots a swap reads: the twelve cue colors it may move, then the
// text colors it keeps them clear of, which never move.
const SWAP_READS: readonly GameSlot[] = [...CUE_SLOTS, ...TEXT_SLOTS];
const READ_AT = new Map<GameSlot, number>(SWAP_READS.map((k, i) => [k, i]));
const readAt = (k: GameSlot) => READ_AT.get(k) as number;

// The grids the swap sweeps: every slot alone over lightness and every
// two slots together, each step in OKLCH L, a turned hue in degrees, and
// the finer steps it refines with, each reaching four steps either way.
const SWEEP_L = 0.01;
const SWEEP_PAIR_L = 0.03;
const SWEEP_HUE = 2.5;
const SWEEP_PASSES = 3;
const REFINE_L = [0.005, 0.0025] as const;
const REFINE_HUE = [-3, -2, -1, -0.5, 0.5, 1, 2, 3] as const;
const REFINE_PASSES = 6;
// Where in its window each turned hue starts, as a share of its reach.
const HUE_STARTS = [0, -1, 1] as const;

// The soft cost of a swap, under every floor: each dE a parted pair
// stands short of its target, a color kept under 75 percent of the
// chroma it aims for, each 0.01 of OKLCH lightness moved and each degree
// of hue off the target.
const SOFT_PART = 10;
const SOFT_CHROMA_SHARE = 0.75;
const SOFT_CHROMA = 400;
const SOFT_L = 0.35;
const SOFT_HUE = 0.1;
// How far apart other cue pairs stand where the room allows.
const CUE_WANT = 6;

// The tiers a swap scores a palette on, firmest first, then the soft
// cost (swapFor). A palette that does better on an earlier tier always
// wins, so a later tier never buys an earlier one.
const HOLD = 0;
const CLEAR = 1;
const SHOW = 2;
const LEAD = 3;
const TURN = 4;
const FLOOR = 5;
const PART = 6;
const WANT = 7;
const ROOM = 8;
const SOFT = 9;
const TIER_COUNT = 10;
const TIE = 1e-9;

function betterTiers(a: readonly number[], b: readonly number[]): boolean {
  for (let i = 0; i < TIER_COUNT; i += 1) {
    if (a[i] < b[i] - TIE) return true;
    if (a[i] > b[i] + TIE) return false;
  }
  return false;
}

// A floor on one pair: where the two slots sit in SWAP_READS, and how
// far apart they stand at the start as the vision sees them and as a
// typical eye does, up to a cap.
type Floor = readonly [number, number, number, number];

interface SwapState {
  L: Float64Array;
  dh: Float64Array;
  // 1 where a color the vision turns keeps its own hue instead.
  own: Uint8Array;
}

interface Scored {
  tiers: number[];
  state: SwapState;
  tones: Tone[];
}

/** Swap the colors `vision` runs together in `src`, a published
 *  palette, starting from `start`, the slots Typical plays over it: the
 *  Typical fit, or nothing for the published colors. Returns the slots
 *  of the swapped palette that differ from `src`. Typical swaps nothing
 *  and returns `start`.
 *
 *  Each family the vision turns (SWAP_TARGETS) takes a hue inside its
 *  window and at least its target chroma, giving up only what sRGB
 *  cannot hold. A search then settles the lightness of all twelve cue
 *  colors and the hue of each turned one, from up to six starts: the
 *  lightness of `start` and the published lightness of the turned
 *  colors, each with the turned hues at their target and at either end
 *  of their window (HUE_STARTS). Where no hue in its window holds the
 *  first two tiers, a turned color keeps its own hue and chroma, as
 *  every other family does, such as tells on a theme whose own text,
 *  yells and cabal already fill the blues. The search scores each
 *  palette on tiers in strict order, so a later tier never buys an
 *  earlier one:
 *
 *  1. Every check `start` passes still passes, and one it misses falls
 *     no more than MISS_SLACK further short (holdsCheck). No color moves
 *     more than L_REACH in lightness, red stays on its side of yellow
 *     and bold yellow where they stood 0.02 apart, a turned color keeps
 *     CHROMA_KEEP of its chroma, or of its target chroma if less, and
 *     every other color HUE_CHROMA_KEEP of its own. Each channel pair
 *     keeps CHANNEL_LEAST, or as far as at the start if that is less,
 *     seen and typical.
 *  2. Each color stays at least VISION_GUARD, or as far as at the start
 *     if that is less, minus VISION_SLACK, from body text, white and
 *     bold white, seen and typical. Each kept pair (KEPT_PAIRS) comes no
 *     nearer, seen, than at the start, minus KEPT_SLACK.
 *  3. The lead color (LEAD_SLOTS), or another plain color the vision
 *     turns, moves at least MOVE_MIN.lead, as a typical eye sees it.
 *  4. The lead color moves at least MOVE_MIN.lead and its bold twin
 *     MOVE_MIN.bold, turned.
 *  5. Every turned color takes a hue in its window rather than keep its
 *     own.
 *  6. Each channel pair keeps CHANNEL_FLOOR and every other cue pair of
 *     a weight CUE_FLOOR, or as far as at the start if that is less,
 *     seen and typical.
 *  7. Each parted pair (PARTED_PAIRS) stands PART_MIN apart, seen, or as
 *     far as a typical eye sees it at the start if that is less, minus
 *     VISION_SLACK.
 *  8. The lead color moves MOVE_WANT.
 *  9. Each channel pair keeps VISION_GUARD and every other cue pair 6,
 *     and each color VISION_GUARD from the text, or as far as at the
 *     start if that is less.
 *
 *  Under them a soft cost keeps each color near its start lightness,
 *  near its target hue and at its chroma. A swap takes one to two and a
 *  half seconds, so a built in theme stores its swaps (themes.ts
 *  VISION_FITS) and a custom theme swaps in a worker (fitOffThread). */
export function swapFor(
  src: XtermPalette,
  vision: ColorVision,
  start: Partial<XtermPalette> = {},
): Partial<XtermPalette> {
  const kind = CVD_OF[vision];
  if (!kind) return start;
  const from: XtermPalette = { ...src, ...start };
  const targets = SWAP_TARGETS[vision];
  const n = CUE_SLOTS.length;

  const tones = new Map<string, Tone>();
  const toneAt = (hex: string) => {
    let tone = tones.get(hex);
    if (!tone) {
      tone = toneOf(hex);
      tones.set(hex, tone);
    }
    return tone;
  };
  const begin = SWAP_READS.map((k) => toneAt(from[k]));
  const lch = CUE_SLOTS.map((k) => rgbToOklch(rgb(from[k])));
  const target = CUE_SLOTS.map((k) => targets[familyOf(k)] ?? null);
  const want = CUE_SLOTS.map((_, i) => Math.max(lch[i].C, target[i]?.chroma ?? 0));
  const keepChroma = CUE_SLOTS.map(
    (_, i) => CHROMA_KEEP * Math.min(lch[i].C, target[i]?.chroma ?? 0),
  );
  const holdChroma = lch.map((c) => (c.C > 0.04 ? HUE_CHROMA_KEEP * c.C : 0));
  const turned = CUE_SLOTS.map((_, i) => i).filter((i) => target[i] !== null);

  // The color slot `i` takes at lightness `L`: for a turned family `dh`
  // degrees off its target hue, at the chroma it aims for, unless it
  // keeps its own hue (`own`), as every other family does.
  const made = CUE_SLOTS.map(() => new Map<number, Tone>());
  const colorAt = (i: number, L: number, dh: number, own: number): Tone => {
    const turn = own ? null : target[i];
    if (!turn && Math.abs(L - lch[i].L) < 1e-9) return begin[i];
    const key = Math.round(L * 1e5) * 4096 + (turn ? Math.round((dh + 100) * 10) : 4095);
    let tone = made[i].get(key);
    if (!tone) {
      const at = Math.max(0, Math.min(1, L));
      const c = turn
        ? oklchToRgbInGamut({ L: at, C: want[i], h: (turn.hue + dh + 360) % 360 })
        : oklchToRgbInGamut({ L: at, C: lch[i].C, h: lch[i].h });
      tone = toneAt(toHex(c));
      made[i].set(key, tone);
    }
    return tone;
  };

  const seenOf = (a: Tone, b: Tone) => labDE(seenTone(a, kind), seenTone(b, kind));
  const typicalOf = (a: Tone, b: Tone) => labDE(a.lab, b.lab);
  const floorsOf = (pairs: readonly (readonly [GameSlot, GameSlot])[], most: number): Floor[] =>
    pairs.map(([a, b]) => {
      const i = readAt(a);
      const j = readAt(b);
      return [
        i,
        j,
        Math.min(seenOf(begin[i], begin[j]), most),
        Math.min(typicalOf(begin[i], begin[j]), most),
      ] as const;
    });
  const textPairs = CUE_SLOTS.flatMap((k) => TEXT_SLOTS.map((t) => [k, t] as const));
  const textFloors = floorsOf(textPairs, VISION_GUARD);
  const channelFloors = floorsOf(CHANNEL_PAIRS, Infinity);
  const cueFloors = floorsOf(CUE_PAIRS, Infinity);
  const kept = KEPT_PAIRS[vision].map(([a, b]) => {
    const i = readAt(a);
    const j = readAt(b);
    return [i, j, seenOf(begin[i], begin[j]) - KEPT_SLACK] as const;
  });
  const parted = PARTED_PAIRS[vision].map(([a, b]) => {
    const i = readAt(a);
    const j = readAt(b);
    return [i, j, Math.min(PART_MIN[vision], typicalOf(begin[i], begin[j]))] as const;
  });
  const [leadSlot, boldSlot] = LEAD_SLOTS[vision].map(readAt);
  // The plain colors the vision turns, which CUE_SLOTS lists first.
  const plainTurned = turned.filter((i) => i < n / 2);
  const red = readAt('red');
  const sides = (['yellow', 'brightYellow'] as const)
    .map((y) => [readAt(y), begin[red].lab.L - begin[readAt(y)].lab.L] as const)
    .filter(([, was]) => Math.abs(was) >= 0.02);

  // The checks as the start has them, and which a swap holds.
  const toneFor =
    (list: readonly Tone[]): ToneOf =>
    (k) => {
      const i = READ_AT.get(k as GameSlot);
      return i === undefined ? toneAt(from[k]) : list[i];
    };
  const held = measure(toneFor(begin));
  const holds = CHECK_SPEC.map(([id]) => holdsCheck(id, vision));
  const ranges = CHECK_SPEC.map(([, need], i): readonly [number, number] => {
    if (i === T4_AT) return [T.dimMin, held.dimTop];
    if (need.startsWith('>=')) return [+need.slice(2), Infinity];
    const [lo, hi] = need.split('..').map(Number);
    return [lo, hi];
  });
  const gapOf = (i: number, value: number) => {
    const v = +value.toFixed(1);
    return Math.max(0, ranges[i][0] - v, v - ranges[i][1]);
  };
  const heldGap = held.values.map((v, i) => gapOf(i, v));

  const score = (state: SwapState): Scored => {
    const list = begin.slice();
    for (let i = 0; i < n; i += 1) list[i] = colorAt(i, state.L[i], state.dh[i], state.own[i]);
    const turnedNow = (i: number) => target[i] !== null && state.own[i] === 0;
    const tiers = new Array<number>(TIER_COUNT).fill(0);
    // 1. The checks, the reach in lightness, red's side of yellow and
    // the chroma each color keeps.
    const now = measure(toneFor(list));
    for (let i = 0; i < now.oks.length; i += 1) {
      if (!holds[i]) continue;
      if (held.oks[i]) {
        if (!now.oks[i]) tiers[HOLD] += 1 + Math.max(0.05, gapOf(i, now.values[i]));
      } else if (!now.oks[i]) {
        const over = gapOf(i, now.values[i]) - heldGap[i];
        if (over > MISS_SLACK) tiers[HOLD] += 1 + over;
      }
    }
    for (let i = 0; i < n; i += 1) {
      const d = Math.abs(list[i].lab.L - begin[i].lab.L);
      if (d > L_REACH) tiers[HOLD] += 1 + (d - L_REACH) * 100;
    }
    for (const [y, was] of sides) {
      const diff = list[red].lab.L - list[y].lab.L;
      if (Math.sign(diff) !== Math.sign(was)) tiers[HOLD] += 1 + Math.abs(diff) * 100;
    }
    for (let i = 0; i < n; i += 1) {
      const floor = turnedNow(i) ? keepChroma[i] : holdChroma[i];
      const C = Math.hypot(list[i].lab.a, list[i].lab.b);
      if (C < floor - TIE) tiers[HOLD] += 1 + (floor - C) * 100;
    }
    // 2 and 9. Clear of the text, and the kept pairs.
    for (const [i, j, seenFloor, typicalFloor] of textFloors) {
      const s = seenOf(list[i], list[j]);
      const t = typicalOf(list[i], list[j]);
      if (s < seenFloor - VISION_SLACK - TIE) tiers[CLEAR] += 1 + (seenFloor - VISION_SLACK - s);
      if (t < typicalFloor - VISION_SLACK - TIE) {
        tiers[CLEAR] += 1 + (typicalFloor - VISION_SLACK - t);
      }
      if (s < seenFloor - TIE) tiers[ROOM] += 1 + (seenFloor - s);
      if (t < typicalFloor - TIE) tiers[ROOM] += 1 + (typicalFloor - t);
    }
    for (const [i, j, floor] of kept) {
      const s = seenOf(list[i], list[j]);
      if (s < floor - TIE) tiers[CLEAR] += 1 + (floor - s);
    }
    // 3, 4, 5 and 8. How far the lead color moves, or the second, and
    // which turned colors keep their own hue. A color that keeps its
    // own hue counts as no move.
    const moved = (i: number) => (turnedNow(i) ? typicalOf(begin[i], list[i]) : 0);
    const lead = moved(leadSlot);
    const bold = moved(boldSlot);
    let shown = lead;
    for (const i of plainTurned) shown = Math.max(shown, moved(i));
    if (shown < MOVE_MIN.lead) tiers[SHOW] += 1 + (MOVE_MIN.lead - shown);
    if (lead < MOVE_MIN.lead) tiers[LEAD] += 1 + (MOVE_MIN.lead - lead);
    if (bold < MOVE_MIN.bold) tiers[LEAD] += 1 + (MOVE_MIN.bold - bold);
    for (const i of turned) tiers[TURN] += state.own[i];
    if (lead < MOVE_WANT) tiers[WANT] += 1 + (MOVE_WANT - lead);
    // 1, 6 and 9. The channel and cue pairs.
    const pairs = (floors: readonly Floor[], least: number, firm: number, soft: number) => {
      for (const [i, j, seenStart, typicalStart] of floors) {
        const s = seenOf(list[i], list[j]);
        const t = typicalOf(list[i], list[j]);
        for (const [tier, most] of [
          [HOLD, least],
          [FLOOR, firm],
          [ROOM, soft],
        ] as const) {
          const sf = Math.min(seenStart, most);
          const tf = Math.min(typicalStart, most);
          if (s < sf - TIE) tiers[tier] += 1 + (sf - s);
          if (t < tf - TIE) tiers[tier] += 1 + (tf - t);
        }
      }
    };
    pairs(channelFloors, CHANNEL_LEAST, CHANNEL_FLOOR, VISION_GUARD);
    pairs(cueFloors, 0, CUE_FLOOR, CUE_WANT);
    // 7. The parted pairs, and their shortfall under the soft cost.
    for (const [i, j, need] of parted) {
      const s = seenOf(list[i], list[j]);
      if (s < need - VISION_SLACK - TIE) tiers[PART] += 1 + (need - VISION_SLACK - s);
      if (s < need) tiers[SOFT] += SOFT_PART * (need - s);
    }
    for (let i = 0; i < n; i += 1) {
      const C = Math.hypot(list[i].lab.a, list[i].lab.b);
      const w = turnedNow(i) ? want[i] : lch[i].C;
      if (w > 0.04 && C < SOFT_CHROMA_SHARE * w) {
        tiers[SOFT] += ((SOFT_CHROMA_SHARE * w - C) / w) * SOFT_CHROMA;
      }
      tiers[SOFT] += Math.abs(state.L[i] - lch[i].L) * 100 * SOFT_L;
      if (turnedNow(i)) tiers[SOFT] += Math.abs(state.dh[i]) * SOFT_HUE;
    }
    return { tiers, state, tones: list };
  };

  const clamp = (L: number) => Math.max(0.02, Math.min(0.995, L));
  // The lightness grid a slot sweeps, inside its reach.
  const gridOf = (i: number, step: number) => {
    const out: number[] = [];
    const lo = Math.max(0.02, lch[i].L - L_REACH - step / 2);
    const hi = Math.min(0.995, lch[i].L + L_REACH + step / 2);
    for (let k = Math.ceil(lo / step); k * step <= hi + 1e-9; k += 1) {
      out.push(+(k * step).toFixed(4));
    }
    return out;
  };
  const sweep = CUE_SLOTS.map((_, i) => gridOf(i, SWEEP_L));
  const sweepPair = CUE_SLOTS.map((_, i) => gridOf(i, SWEEP_PAIR_L));
  // The bold twin of each plain cue color, and the plain twin of each
  // bold, which CUE_SLOTS lists half a row apart.
  const twinAt = CUE_SLOTS.map((_, i) => (i + n / 2) % n);

  const run = (first: SwapState): Scored => {
    let best = score(first);
    const consider = (L: Float64Array, dh: Float64Array, own = best.state.own) => {
      const next = score({ L, dh, own });
      if (!betterTiers(next.tiers, best.tiers)) return false;
      best = next;
      return true;
    };
    const withL = (i: number, L: number, j = -1, Lj = 0) => {
      const out = best.state.L.slice();
      out[i] = L;
      if (j >= 0) out[j] = Lj;
      return out;
    };
    const withHue = (i: number, d: number) => {
      const out = best.state.dh.slice();
      out[i] = d;
      return out;
    };
    for (let pass = 0; pass < SWEEP_PASSES; pass += 1) {
      let moved = false;
      for (let i = 0; i < n; i += 1) {
        for (const L of sweep[i]) {
          if (L !== best.state.L[i]) moved = consider(withL(i, L), best.state.dh) || moved;
        }
      }
      for (const i of turned) {
        // Keep its own hue, or turn it again, at each lightness.
        const own = best.state.own.slice();
        own[i] = 1 - own[i];
        const dh = withHue(i, 0);
        for (const L of sweep[i]) moved = consider(withL(i, L), dh, own) || moved;
        if (best.state.own[i]) continue;
        const reach = target[i]?.reach ?? 0;
        for (let d = -reach; d <= reach + 1e-9; d += SWEEP_HUE) {
          if (d !== best.state.dh[i]) moved = consider(best.state.L, withHue(i, d)) || moved;
        }
        // The hue and the lightness together, since a turned color may
        // need both to clear its neighbors.
        for (let d = -reach; d <= reach + 1e-9; d += SWEEP_HUE) {
          for (const L of sweepPair[i]) moved = consider(withL(i, L), withHue(i, d)) || moved;
        }
        // And with the lightness of its twin, which the bold step ties
        // to it.
        const j = twinAt[i];
        for (let d = -reach; d <= reach + 1e-9; d += SWEEP_HUE) {
          for (const Li of sweepPair[i]) {
            for (const Lj of sweepPair[j]) {
              moved = consider(withL(i, Li, j, Lj), withHue(i, d)) || moved;
            }
          }
        }
      }
      for (let i = 0; i < n; i += 1) {
        for (let j = i + 1; j < n; j += 1) {
          for (const Li of sweepPair[i]) {
            for (const Lj of sweepPair[j]) {
              moved = consider(withL(i, Li, j, Lj), best.state.dh) || moved;
            }
          }
        }
      }
      if (!moved) break;
    }
    for (const step of REFINE_L) {
      for (let pass = 0; pass < REFINE_PASSES; pass += 1) {
        let moved = false;
        for (let i = 0; i < n; i += 1) {
          for (let k = -4; k <= 4; k += 1) {
            if (k === 0) continue;
            moved = consider(withL(i, clamp(best.state.L[i] + k * step)), best.state.dh) || moved;
          }
        }
        for (const i of turned) {
          if (best.state.own[i]) continue;
          const reach = target[i]?.reach ?? 0;
          for (const d of REFINE_HUE) {
            const dh = Math.max(-reach, Math.min(reach, best.state.dh[i] + d));
            if (dh !== best.state.dh[i]) moved = consider(best.state.L, withHue(i, dh)) || moved;
          }
        }
        if (!moved) break;
      }
    }
    return best;
  };

  // The starts: the lightness Typical plays and the published lightness
  // of each turned color, each with the turned colors at their target
  // hue and at either end of their window.
  const home = Float64Array.from(lch, (c) => c.L);
  const published = Float64Array.from(CUE_SLOTS, (k, i) =>
    target[i] ? rgbToOklch(rgb(src[k])).L : lch[i].L,
  );
  const lights = published.some((L, i) => L !== home[i]) ? [home, published] : [home];
  const runs = lights.flatMap((L) =>
    HUE_STARTS.map((side) =>
      run({
        L,
        dh: Float64Array.from(target, (t) => (t ? side * t.reach : 0)),
        own: new Uint8Array(n),
      }),
    ),
  );
  const best = runs.reduce((win, r) => (betterTiers(r.tiers, win.tiers) ? r : win));
  const p = { ...from };
  CUE_SLOTS.forEach((k, i) => {
    p[k] = best.tones[i].hex;
  });
  return movedFrom(src, p);
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
 *  the fitted palette says what is short.
 *
 *  Typical is the best of six searches, three seeds each from the
 *  published colors and from tune(), exactly as before color vision. It
 *  takes about two seconds, so a built in theme stores its result.
 *
 *  Another vision swaps the colors it runs together (swapFor), from
 *  `typical`, the slots Typical plays: the Typical fit, or nothing for
 *  the published colors. Without it the swap starts from a Typical fit
 *  worked out here. */
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
  return swapFor(src, vision, typical ?? fit(src));
}
