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
// A color vision other than Typical gets fits of its own, each the
// Typical fit with the colors of the pairs the vision raises moved
// further apart. A built in theme ships them in themes.ts (VISION_FITS)
// where the fit for a vision moves anything, and gameFit.test.ts fits
// them again with VOSH_FIT_THEMES=1.

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
function simulate(hex: string, kind: Cvd): [number, number, number] {
  const v = rgbToLinear(rgb(hex));
  const [r, g, b] = MACHADO[kind].map((row) =>
    Math.max(0, Math.min(1, row[0] * v[0] + row[1] * v[1] + row[2] * v[2])),
  );
  return [r, g, b];
}

function seenAs(hex: string, kind: Cvd) {
  return linearToOklab(simulate(hex, kind));
}

const dECvd = (a: string, b: string, kind: Cvd) => {
  const p = seenAs(a, kind);
  const q = seenAs(b, kind);
  return 100 * Math.hypot(p.L - q.L, p.a - q.a, p.b - q.b);
};

// ── Color vision ───────────────────────────────────────────────────

/** The color vision play fits the game colors for, from UiConfig
 *  color_vision. Typical asks the 46 checks as they stand. Each other
 *  vision raises the floors of the T7 pairs its own simulation
 *  measures, red against yellow, bright yellow and green for a
 *  deuteranope or a protanope, cyan against green for a tritanope. */
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
  // A color vision asks the T7 pairs its own simulation measures to
  // stand a quarter farther apart. The review's floors already hold for
  // a color blind player, and this gives the player who picks the
  // vision a margin past them. Where lightness cannot reach it without
  // giving up a check the Typical fit passes, the pair stays as far
  // apart as it gets, and themes.test.ts names those pairs.
  visionRaise: 1.25,
};

// The floor of a T7 pair measured through `kind`, raised for the vision
// that sees through it.
const cvdNeed = (need: number, kind: Cvd, vision: ColorVision) =>
  CVD_OF[vision] === kind ? need * T.visionRaise : need;

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

/** Every check, in the order the review lists them. A color vision
 *  other than Typical raises the floors of the T7 pairs its own
 *  simulation measures. */
export function checks(p: XtermPalette, vision: ColorVision = 'typical'): GameCheck[] {
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
    for (const [a, b, floor] of CVD_PAIRS) {
      const need = cvdNeed(floor, kind, vision);
      const v = dECvd(p[a], p[b], kind);
      add(`T7 ${kind} ${a}/${b}`, v, `>=${need}`, v >= need);
    }
  }
  add('T7 red/yellow dL', dL(p.red, p.yellow), '>=10', dL(p.red, p.yellow) >= T.rySep);
  add('T7 red/brightRed dL', dL(p.red, p.brightRed), '>=8', dL(p.red, p.brightRed) >= T.rRbDL);
  const tc = dECvd(p.cyan, p.green, 'tritan');
  const tcNeed = cvdNeed(T.tritanCG, 'tritan', vision);
  add('T7 tritan cyan/green', tc, `>=${tcNeed}`, tc >= tcNeed);
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

// The T7 pairs `kind` measures, each with its floor before a vision
// raises it.
const pairsOf = (kind: Cvd): readonly (readonly [AnsiSlot, AnsiSlot, number])[] =>
  kind === 'tritan' ? [['cyan', 'green', T.tritanCG]] : CVD_PAIRS;

// How far short of the floors a color vision raises its own pairs fall,
// summed the same way. Typical raises none.
function visionShortfall(p: XtermPalette, vision: ColorVision): number {
  const kind = CVD_OF[vision];
  if (!kind) return 0;
  let s = 0;
  for (const [a, b, floor] of pairsOf(kind)) {
    const need = cvdNeed(floor, kind, vision);
    const v = dECvd(p[a], p[b], kind);
    if (v < need) s += (need - +v.toFixed(1)) / need + 0.05;
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
// leaves short, such as a dim red, gets no fainter.
function keeps(p: XtermPalette, held: readonly GameCheck[]): boolean {
  return checks(p).every((c, i) => (held[i].ok ? c.ok : gap(c) <= gap(held[i])));
}

/** The slots a fit for `vision` may move, the slots of the pairs it
 *  raises and the bold or plain twin of each, which T6 ties to it.
 *  Every other slot keeps its Typical fit. Typical names none. */
export function visionSlots(vision: ColorVision): readonly GameSlot[] {
  const kind = CVD_OF[vision];
  if (!kind) return [];
  const named = new Set<GameSlot>();
  for (const [a, b] of pairsOf(kind)) {
    for (const k of [a, b]) {
      for (const pair of PAIRS) if (pair.includes(k)) pair.forEach((s) => named.add(s));
    }
  }
  return GAME_SLOTS.filter((k) => named.has(k));
}

const ITERS = 9000;
const SEEDS = [11, 23, 37];
const MOVE_WEIGHT = 0.35;

type Lightness = Record<GameSlot, number>;

interface Search {
  cost: number;
  palette: XtermPalette;
}

// What a search for a color vision holds to.
interface Hold {
  vision: ColorVision;
  // The Typical fit laid over the published colors. The search starts
  // here and measures each move from here.
  from: XtermPalette;
  // checks() of `from`, the floors no step may give up.
  held: readonly GameCheck[];
  // The slots the search may move (visionSlots).
  slots: readonly GameSlot[];
}

// One random search over the slots' lightness, from `start` or from
// the published colors. It keeps a step only when the cost drops. The
// cost is the shortfall first, then the total distance from the
// published colors, and a heavy charge on any slot that loses more
// than 40 percent of its chroma, so a yellow never turns cream.
//
// With `hold` it fits for a color vision. It starts from the Typical
// fit, moves only the slots `hold` names, and takes no step that gives
// up a check the Typical fit passes or falls further short of one it
// misses. The cost is how far the floors the vision raises fall short,
// then the total distance from the Typical fit, and the same charge on
// chroma.
function search(
  src: XtermPalette,
  seed: number,
  start: XtermPalette | undefined,
  hold?: Hold,
): Search {
  // A linear congruential generator in plain doubles. The products
  // pass 2^53 and lose low bits, and the fit depends on exactly those
  // values, so integer math would draw a different sequence.
  let s = seed;
  const rnd = () => {
    s = (s * 1103515245 + 12345) % 2147483648;
    return s / 2147483648;
  };
  const base = Object.fromEntries(GAME_SLOTS.map((k) => [k, rgbToOklch(rgb(src[k]))])) as Record<
    GameSlot,
    Oklch
  >;
  // Where each slot starts and where a pull back heads, the published
  // colors for Typical and the Typical fit for a color vision.
  const origin = hold?.from ?? src;
  const home = Object.fromEntries(
    GAME_SLOTS.map((k) => [k, hold ? rgbToOklch(rgb(origin[k])) : base[k]]),
  ) as Record<GameSlot, Oklch>;
  const at = (k: GameSlot, L: number) => {
    const o = base[k];
    if (Math.abs(L - o.L) < 1e-9) return { hex: src[k], C: o.C };
    if (Math.abs(L - home[k].L) < 1e-9) return { hex: origin[k], C: home[k].C };
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
    for (const k of GAME_SLOTS) move += dE(origin[k], p[k]);
    let missed: number;
    if (!hold) missed = shortfall(p) * 100;
    else if (keeps(p, hold.held)) missed = visionShortfall(p, hold.vision) * 100;
    else return { cost: Infinity, palette: p };
    return { cost: missed + (move * MOVE_WEIGHT) / 10 + drained * 400, palette: p };
  };
  let cur = Object.fromEntries(
    GAME_SLOTS.map((k) => [k, start ? rgbToOklch(rgb(start[k])).L : base[k].L]),
  ) as Lightness;
  let best = cost(cur);
  const slots = hold?.slots ?? GAME_SLOTS;
  for (let i = 0; i < ITERS; i += 1) {
    const t = { ...cur };
    const amp = 0.06 * (1 - i / ITERS) + 0.004;
    // For a color vision, half the moves step a few slots by the same
    // amount, so colors a floor ties together, such as yellow and bright
    // yellow, move as one where each alone would break it.
    if (hold && rnd() < 0.5) {
      const d = (rnd() - 0.5) * 2 * amp;
      for (const k of slots) if (rnd() < 0.5) t[k] = Math.max(0, Math.min(1, t[k] + d));
    } else {
      const n = 1 + Math.floor(rnd() * 3);
      for (let j = 0; j < n; j += 1) {
        const k = slots[Math.floor(rnd() * slots.length)];
        // A quarter of the moves pull a slot back toward where it
        // started, its published lightness for Typical.
        const L0 = home[k].L;
        t[k] = rnd() < 0.25 ? L0 + (t[k] - L0) * rnd() : t[k] + (rnd() - 0.5) * 2 * amp;
        t[k] = Math.max(0, Math.min(1, t[k]));
      }
    }
    const c = cost(t);
    if (c.cost < best.cost) {
      best = c;
      cur = t;
    }
  }
  return best;
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
export function needsFit(p: XtermPalette, vision: ColorVision = 'typical'): boolean {
  try {
    return checks(p, vision).some((c) => !c.ok);
  } catch {
    return false;
  }
}

/** Whether `p` holds every floor `vision` raises. Typical raises none. */
export function holdsVision(p: XtermPalette, vision: ColorVision): boolean {
  return visionShortfall(p, vision) === 0;
}

// The slots of `p` that differ from `src`.
function movedFrom(src: XtermPalette, p: XtermPalette): Partial<XtermPalette> {
  const moved: Partial<XtermPalette> = {};
  for (const k of GAME_SLOTS) if (p[k] !== src[k]) moved[k] = p[k];
  return moved;
}

// The first of the cheapest wins.
const cheapest = (runs: Search[]) =>
  runs.reduce((win, run) => (run.cost < win.cost ? run : win)).palette;

/** Fit a published palette to the checks, for `vision`, and return only
 *  the slots it moved. What still misses stays missed, so checks() on
 *  the fitted palette for the same vision says what is short.
 *
 *  Typical is the best of six searches, three seeds each from the
 *  published colors and from tune(), exactly as before color vision. It
 *  takes about two seconds, so a built in theme stores its result.
 *
 *  Another vision builds on the Typical fit, `typical` when you have it.
 *  It keeps that fit when it already holds the floors the vision raises.
 *  Else three searches start from it and move only the slots of the
 *  raised pairs and their twins (visionSlots), so every other color
 *  plays as Typical has it. No step gives up a check the Typical fit
 *  passes or falls further short of one it misses, so where lightness
 *  cannot part a pair without that, the pair stays as far apart as it
 *  gets. Where no step parts a pair at all, the fit is the Typical fit.
 *  That takes about a second. */
export function fit(
  src: XtermPalette,
  vision: ColorVision = 'typical',
  typical?: Partial<XtermPalette>,
): Partial<XtermPalette> {
  if (vision === 'typical') {
    const runs = [undefined, tune(src)].flatMap((start) =>
      SEEDS.map((seed) => search(src, seed, start)),
    );
    return movedFrom(src, cheapest(runs));
  }
  const base = typical ?? fit(src);
  const from = { ...src, ...base };
  if (holdsVision(from, vision)) return base;
  const hold: Hold = { vision, from, held: checks(from), slots: visionSlots(vision) };
  const p = cheapest(SEEDS.map((seed) => search(src, seed, from, hold)));
  // Every step that parts a raised pair further gives something up.
  if (GAME_SLOTS.every((k) => p[k] === from[k])) return base;
  return movedFrom(src, p);
}
