// Chrome tokens derived from a terminal palette. The window chrome
// (panel, lines, text tiers, status colors) follows the terminal theme
// by a fixed rule instead of a hand authored second palette, so any
// imported terminal theme dresses the whole window. This is the one
// ground rule:
//
//   appearance  OKLab L of the background below 0.6 is dark
//   panel       the terminal ground itself, so the window is one surface
//   lines       steps in OKLab L, white over a dark ground and black
//               over a light one. The line (sep) steps 11, the divider
//               6, hover 4 and a dark selected row 7, all over the
//               panel, and the input band 2.5 over the ground. OKLab L
//               runs so steep near pure black that a step of 11 lands
//               on #040404 there, so on a dark ground each step keeps
//               at least the alpha it takes on Obsidian Ember's #050403.
//   raised      a white step of 6 on dark. On light, the paper lifted
//               0.03 in OKLCH L at its own hue with half its chroma, so
//               nothing floats or selects on pure white, and a light
//               selected row is the raised paper.
//   text tiers  the foreground, lifted at its hue to 6.0:1 on the panel
//               when it reads under that, then the foreground over the
//               panel at the alpha that reaches 6.0:1 (secondary) and
//               3.1:1 (tertiary)
//   title       the secondary tier. frame.css dims it to tertiary while
//               the window is unfocused.
//   status      ANSI red, yellow, green lifted at their hue to 3:1
//   danger text danger lifted to 4.5:1, for words drawn in danger,
//               while markers, dots, and meter fills keep danger
//   accent      the cursor when it carries color, is not the text, and
//               stands 12 dE from danger, warn and success. Else the
//               scheme's green, yellow, blue, magenta or cyan, normal or
//               bright, with the most chroma that stands as far. Else
//               bright blue. Lifted at its hue to 3:1 on the panel.
//   on accent   white, or the theme's dark end lifted down to 4.5:1
//   selection   the scheme's own selection, opaque, when it stands a step
//               of 6 off the ground and its own selection text, else the
//               foreground, reads 4.5:1 on it. Else the accent over the
//               ground at 0.28 on dark and 0.20 on light, with the text
//               tier on it. The selection text token travels with the
//               fill, so the window and both renderers draw one pair.
//   controls    the field, the off switch track, the keycap ring, the
//               menu highlight and the edge are washes, white on dark
//               and black on light, at the alpha that steps the surface
//               each sits on by WASH_STEP, with the same floor near
//               black. A wash, so a field takes its step on the panel
//               and on a raised card alike. A light field is the raised
//               paper itself, never white.
//
// Tertiary and the status colors also draw on raised surfaces (menu
// shortcuts, palette keycaps, a danger row), so their floors hold on
// whichever of the panel and raised reads worse.
//
// A theme can pin any token with an override, and a pin wins as it
// stands. Overridden base colors (bg, panel, raised, text, the status
// colors) feed the tokens derived from them.
//
// A color vision other than Typical swaps the status colors after the
// rule above (statusSeenBy), the way the game text swaps: under
// deuteranopia and protanopia success turns blue and danger toward
// vermilion, clear of the text and secondary tiers the marks sit beside.
// Tritanopia keeps their hues, and moves them lighter or darker only
// where a tritanope sees danger near warn or success. The text tones,
// the accent and the selection then derive from the swapped colors, and
// the accent the rule picks stands ACCENT_APART from each as that vision
// sees them where a hue of the theme does.

import {
  BLACK,
  WHITE,
  composite,
  contrast,
  deltaEOk,
  liftAtHue,
  oklabToRgb,
  oklchToRgbInGamut,
  parseHex,
  rgbToOklab,
  rgbToOklch,
  shiftLightness,
  solveAlphaForContrast,
  toHex,
  toRgba,
  type Rgb,
} from './color';
import {
  CHROMA_KEEP,
  HUE_CHROMA_KEEP,
  KEPT_SLACK,
  seenApart,
  seenLab,
  VISION_GUARD,
  VISION_SLACK,
  type ColorVision,
  type SwapTarget,
} from './gameFit';
import type { XtermPalette } from './themes';

export type Appearance = 'dark' | 'light';

export interface ChromeTokens {
  appearance: Appearance;
  /// Terminal ground. The title band and input band sit on it.
  bg: string;
  /// Panel ground, the terminal ground unless a theme pins its own.
  panel: string;
  /// Strong hairline: the panel edge and split handles.
  sep: string;
  /// Soft hairline between sections inside a pane. On dark it steps
  /// the panel as far as raised does, so the two are one color, and a
  /// line or ring on a floating surface takes sep or keyRing instead.
  divider: string;
  /// Selected row fill.
  selrow: string;
  /// Hovered row fill.
  hover: string;
  /// Command input band fill.
  inputband: string;
  /// Primary text, the terminal foreground.
  text: string;
  /// Row text and labels, 6:1 on the panel.
  secondary: string;
  /// Meta, captions, and caps labels, 3.1:1 on the panel.
  tertiary: string;
  /// Window title in the title band, the secondary tier.
  title: string;
  /// Floating surfaces: the palette, menus, dialogs.
  raised: string;
  accent: string;
  /// Text drawn on an accent fill.
  onAccent: string;
  /// Markers, dots, and meter fills that flag trouble, 3:1 on the panel.
  danger: string;
  /// Words drawn in the danger tone, 4.5:1 on the panel.
  dangerText: string;
  /// Markers, dots, and rings that warn, 3:1 on the panel.
  warn: string;
  /// Words drawn in the warn tone, 4.5:1 on the panel.
  warnText: string;
  success: string;
  /// Terminal selection, opaque.
  selection: string;
  /// Text drawn on the selection.
  selectionText: string;
  /// Control fill: text fields, selects, chips, and the segmented
  /// track on dark.
  field: string;
  /// A switch track while the switch is off.
  track: string;
  /// The hovered or keyboard row on a floating surface.
  menuHi: string;
  /// Keycap ring, and the ring inside a color swatch, in Settings, on
  /// the prompt card and in the pane menus.
  keyRing: string;
  /// Window edge, the ring inside a floating surface on dark, and the
  /// edge a band on paper draws inside itself to read on its ground.
  edge: string;
}

export type ChromeOverrides = Partial<ChromeTokens>;

/** Every color token in the order tokens.css declares them. */
export const CHROME_COLOR_KEYS = [
  'bg',
  'panel',
  'sep',
  'divider',
  'selrow',
  'hover',
  'inputband',
  'text',
  'secondary',
  'tertiary',
  'title',
  'raised',
  'accent',
  'onAccent',
  'danger',
  'dangerText',
  'warn',
  'warnText',
  'success',
  'selection',
  'selectionText',
  'field',
  'track',
  'menuHi',
  'keyRing',
  'edge',
] as const satisfies readonly (keyof ChromeTokens)[];

export type ChromeColorKey = (typeof CHROME_COLOR_KEYS)[number];

export const APPEARANCE_THRESHOLD = 0.6;
export const SECONDARY_CONTRAST = 6.0;
export const TERTIARY_CONTRAST = 3.1;
export const STATUS_CONTRAST = 3.0;
export const STATUS_TEXT_CONTRAST = 4.5;
export const ON_ACCENT_CONTRAST = 4.5;
/// The accent, lifted at its hue to this contrast on the panel.
const ACCENT_CONTRAST = 3.0;
/// OKLCH chroma above which the cursor counts as a color of its own.
const ACCENT_CHROMA = 0.05;
/// An accent the rule picks stands this far from danger, warn and
/// success, in OKLab dE times 100.
export const ACCENT_APART = 12;
/// Darkest ink on an accent fill never sits above this OKLab L.
const INK_MAX_L = 0.24;

/// The lightness steps, in OKLab L times 100, that the lines and fills
/// take off the ground they draw over. The line steps 11 so it stands
/// 5 off a menu, which steps 6.
const STEP = {
  sep: 11,
  divider: 6,
  hover: 4,
  selrow: 7,
  inputband: 2.5,
  raised: 6,
} as const;
type Step = keyof typeof STEP;

/// A light theme floats and selects on its paper lifted this far in
/// OKLCH L, at its own hue with half its chroma, and no lighter than
/// RAISED_LIGHT_MAX_L.
const RAISED_LIGHT_LIFT = 0.03;
const RAISED_LIGHT_MAX_L = 0.995;

/// Obsidian Ember's ground. Below it OKLab L runs too steep to step by,
/// so a step on a dark ground keeps at least the alpha it takes here.
const NEAR_BLACK: Rgb = { r: 5, g: 4, b: 3 };

/// The scheme's own selection draws when it stands this far off the
/// ground, in OKLab L times 100, and its text reads this well on it.
const SELECTION_STEP = 6;
const SELECTION_TEXT_CONTRAST = 4.5;
/// Otherwise the selection is the accent over the ground at this alpha.
const SELECTION_ALPHA = { dark: 0.28, light: 0.2 } as const;

/// The steps the control washes take off the surface they sit on, in
/// OKLab L times 100: the field, the track and the keycap ring off the
/// panel, the menu highlight off raised, and the edge off the ground.
/// Each is the step the fixed wash before them took there, white on
/// Obsidian Ember and black on Vellum, so those two paint as they did.
/// On dark the edge takes the 0.12 of the ring inside a floating
/// surface. The window edge on Windows and Linux took 0.10 in the main
/// window and 0.18 in Settings, so one edge moves each of them on
/// Ember. On light every edge took 0.14. A light field is the raised
/// paper, so it has no step.
export const WASH_STEP = {
  dark: {
    field: 7.94, // white 0.06
    track: 18.57, // white 0.16
    keyRing: 16.53, // white 0.14
    menuHi: 8.35, // white 0.08
    edge: 14.46, // white 0.12
  },
  light: {
    track: 10.39, // black 0.14
    keyRing: 8.88, // black 0.12
    menuHi: 3.86, // black 0.05
    edge: 10.39, // black 0.14
  },
} as const;

/// The scheme's own hues the accent may come from, normal and bright.
/// Red stays out, since red in the window means trouble.
const ACCENT_SLOTS = [
  'green',
  'brightGreen',
  'yellow',
  'brightYellow',
  'blue',
  'brightBlue',
  'magenta',
  'brightMagenta',
  'cyan',
  'brightCyan',
] as const satisfies readonly (keyof XtermPalette)[];

const FALLBACK_BG: Rgb = { r: 16, g: 18, b: 24 };
const FALLBACK_FG: Rgb = { r: 204, g: 204, b: 204 };
const FALLBACK_BLUE: Rgb = { r: 97, g: 175, b: 239 };

function hexOr(value: string | undefined, fallback: Rgb): Rgb {
  return (value !== undefined && parseHex(value)) || fallback;
}

// An override keeps its exact string in the output. When it parses as
// hex it also replaces the derived color for anything built on it.
function pick(override: string | undefined, derived: Rgb): { css: string; rgb: Rgb } {
  if (override === undefined || override === '') return { css: toHex(derived), rgb: derived };
  return { css: override, rgb: parseHex(override) ?? derived };
}

const lightness = (c: Rgb) => rgbToOklab(c).L * 100;

/** The alpha, found by a binary search, at which `wash` over `ground`
 *  sits `dl` away from it in OKLab L. */
function washAlpha(wash: Rgb, ground: Rgb, dl: number): number {
  const base = lightness(ground);
  let lo = 0;
  let hi = 1;
  for (let i = 0; i < 30; i += 1) {
    const a = (lo + hi) / 2;
    if (Math.abs(lightness(composite(wash, ground, a)) - base) < dl) lo = a;
    else hi = a;
  }
  return hi;
}

/** The lightness step between two colors, in OKLab L times 100. Below
 *  NEAR_BLACK, where OKLab L runs too steep, a step counts as the one
 *  that gives the same contrast on NEAR_BLACK. */
function stepDL(a: Rgb, b: Rgb): number {
  const base = lightness(NEAR_BLACK);
  if (Math.min(lightness(a), lightness(b)) >= base) return Math.abs(lightness(a) - lightness(b));
  const ratio = contrast(a, b);
  if (ratio <= 1) return 0;
  let lo = 0;
  let hi = 1;
  for (let i = 0; i < 30; i += 1) {
    const alpha = (lo + hi) / 2;
    if (contrast(composite(WHITE, NEAR_BLACK, alpha), NEAR_BLACK) < ratio) lo = alpha;
    else hi = alpha;
  }
  return lightness(composite(WHITE, NEAR_BLACK, hi)) - base;
}

/// The alpha each step takes on NEAR_BLACK, worked out once.
const NEAR_BLACK_ALPHA = Object.fromEntries(
  Object.entries(STEP).map(([key, dl]) => [key, washAlpha(WHITE, NEAR_BLACK, dl)]),
) as Record<Step, number>;

/** The step named `key` over `ground`: white on dark, black on light. */
function stepOver(ground: Rgb, key: Step, dark: boolean): Rgb {
  const wash = dark ? WHITE : BLACK;
  const alpha = washAlpha(wash, ground, STEP[key]);
  return composite(wash, ground, dark ? Math.max(alpha, NEAR_BLACK_ALPHA[key]) : alpha);
}

/** A wash that steps `ground` by `dl` in OKLab L, as rgba(): white on
 *  dark, black on light. On dark it keeps at least the alpha the same
 *  step takes on NEAR_BLACK. */
function washOver(ground: Rgb, dl: number, dark: boolean): string {
  const wash = dark ? WHITE : BLACK;
  const alpha = washAlpha(wash, ground, dl);
  return toRgba(wash, dark ? Math.max(alpha, washAlpha(WHITE, NEAR_BLACK, dl)) : alpha);
}

/** Move `c` in OKLab lightness, away from `against`, until it reaches
 *  `target` contrast. Hue and chroma hold. Returns `c` unchanged when
 *  it already meets the target. */
export function liftToContrast(c: Rgb, against: Rgb, target: number, dir: 1 | -1): Rgb {
  if (contrast(c, against) >= target) return c;
  const lab = rgbToOklab(c);
  let out = c;
  for (let L = lab.L; L >= 0 && L <= 1; L += dir * 0.005) {
    const raw = oklabToRgb({ ...lab, L });
    out = { r: Math.round(raw.r), g: Math.round(raw.g), b: Math.round(raw.b) };
    if (contrast(out, against) >= target) break;
  }
  return out;
}

export function appearanceOf(bg: Rgb): Appearance {
  return rgbToOklab(bg).L < APPEARANCE_THRESHOLD ? 'dark' : 'light';
}

/** The accent the rule picks before any lift: the colored cursor, else
 *  the scheme hue with the most chroma, each only where it stands
 *  ACCENT_APART from every status color, else bright blue. For a color
 *  vision other than Typical a hue stands apart where the accent it
 *  lifts to (`lift`) does, as that vision sees it. The rule keeps the
 *  Typical pick (`prefer`) where it stands apart, and where no hue does
 *  it takes the one that stands farthest. */
function accentCandidate(
  x: XtermPalette,
  status: Rgb[],
  vision: ColorVision = 'typical',
  lift: (c: Rgb) => Rgb = (c) => c,
  prefer?: Rgb,
): Rgb {
  const apart =
    vision === 'typical'
      ? (c: Rgb) => status.every((s) => deltaEOk(c, s) >= ACCENT_APART)
      : (c: Rgb) => status.every((s) => seenApart(lift(c), s, vision) >= ACCENT_APART);
  if (prefer && apart(prefer)) return prefer;
  const cursor = parseHex(x.cursor);
  if (
    cursor !== null &&
    rgbToOklch(cursor).C > ACCENT_CHROMA &&
    toHex(cursor) !== toHex(hexOr(x.foreground, FALLBACK_FG)) &&
    apart(cursor)
  ) {
    return cursor;
  }
  const hues = ACCENT_SLOTS.map((slot) => parseHex(x[slot]))
    .filter((c): c is Rgb => c !== null)
    .sort((a, b) => rgbToOklch(b).C - rgbToOklch(a).C);
  const found = hues.find(apart);
  if (found || vision === 'typical') return found ?? hexOr(x.brightBlue, FALLBACK_BLUE);
  // Where no hue stands apart for the vision, the one that stands
  // farthest from its nearest status color, the Typical pick and the
  // cursor among them.
  const nearest = (c: Rgb) => Math.min(...status.map((s) => seenApart(lift(c), s, vision)));
  const all = [...(prefer ? [prefer] : []), ...hues, hexOr(x.brightBlue, FALLBACK_BLUE)];
  if (cursor !== null && rgbToOklch(cursor).C > ACCENT_CHROMA) all.push(cursor);
  return all.reduce((win, c) => (nearest(c) > nearest(win) ? c : win));
}

// ── The status colors for a color vision ───────────────────────────

interface Picked {
  css: string;
  rgb: Rgb;
}

export type StatusKey = 'danger' | 'warn' | 'success';

// Success turns blue and danger toward vermilion, the hues the game text
// takes for tells and hits. Warn keeps its hue.
const RED_GREEN_STATUS: Readonly<Partial<Record<StatusKey, SwapTarget>>> = {
  danger: { hue: 45, reach: 15, chroma: 0.13 },
  success: { hue: 230, reach: 25, chroma: 0.11 },
};

/// The status colors each vision turns, the way the game text swaps
/// (gameFit SWAP_TARGETS). Tritanopia turns none, since a tritanope
/// tells red, yellow and green apart by hue, and only moves them in
/// lightness where they stand near (statusApart). Typical turns none.
export const STATUS_SWAP: Readonly<
  Record<ColorVision, Readonly<Partial<Record<StatusKey, SwapTarget>>>>
> = {
  typical: {},
  deuteranopia: RED_GREEN_STATUS,
  protanopia: RED_GREEN_STATUS,
  tritanopia: {},
};

/// How far success moves at least where a vision turns it, in OKLab dE
/// times 100 as a typical eye sees it, so the swap shows.
export const STATUS_MOVE_MIN = 10;

/// How far apart danger and success stand at least as the vision sees
/// them, or as far as a typical eye sees the Typical colors if that is
/// less, give or take VISION_SLACK. Under tritanopia danger stands as far
/// from warn too.
export const STATUS_PART = 20;

/// The steps the status colors take for a color vision: lightness in
/// OKLCH L, up to STATUS_REACH either way, and hue in degrees inside a
/// turned color's window.
const STATUS_STEP = 0.01;
const STATUS_REACH = 0.4;
const STATUS_HUE_STEP = 2.5;
/// A turned status color keeps CHROMA_KEEP of its own chroma, or of the
/// chroma it aims for if that is less, and a status color that keeps its
/// hue HUE_CHROMA_KEEP of its own (gameFit), so no distance is bought by
/// lifting a color to white or sinking it to brown.
/// What the solve weighs, firmest first: standing too near a pinned
/// accent, danger coming nearer warn than under Typical or warn nearer
/// success than its floor, success moving too little to see, standing
/// too near a picked accent, and danger short of its distance from
/// success, and under tritanopia from warn. Under them each dE of move
/// costs 0.035, each degree off the target hue 0.02 and each dE danger
/// stands short of success or warn 10.
const PINNED_COST = 1e8;
const PAIR_COST = 1e7;
const MOVE_SHORT_COST = 1e6;
const PICKED_COST = 1e5;
const PART_COST = 1e4;
const MOVE_COST = 0.035;
const HUE_COST = 0.02;
const PART_SOFT = 10;

/// The accent the status colors keep clear of: the theme's pin, or the
/// one the rule picked for Typical.
interface StatusAccent {
  rgb: Rgb;
  pinned: boolean;
}

interface StatusOption {
  rgb: Rgb;
  seen: { L: number; a: number; b: number };
  cost: number;
}

const seenDistance = (p: StatusOption['seen'], q: StatusOption['seen']) =>
  100 * Math.hypot(p.L - q.L, p.a - q.a, p.b - q.b);

const hueOff = (h: number, target: number) => Math.abs(((h - target + 540) % 360) - 180);

// How near the accent a status color may come, as `vision` sees them:
// as near as the Typical color stands, up to ACCENT_APART, and for the
// rule's pick no nearer than the Typical color stands to a typical eye.
function accentNeedOf(accent: StatusAccent, c: Rgb, vision: ColorVision): number {
  return Math.min(
    ACCENT_APART,
    deltaEOk(accent.rgb, c),
    accent.pinned ? seenApart(accent.rgb, c, vision) : Infinity,
  );
}

// Every color a status color may take for `vision`: its own lightness
// stepped either way and, for a color the vision turns (`target`), every
// hue inside its window at the chroma it aims for. Each keeps the
// contrast the Typical color holds on every ground, up to the 3:1
// floor, keeps its chroma (CHROMA_KEEP, HUE_CHROMA_KEEP), and stands as
// far from the text the marks sit beside (`marks`) as the Typical color
// stands, up to VISION_GUARD, both as the vision sees them and as a
// typical eye does. Each carries its cost: its move, its hue off the
// target, standing too near the accent, and for a color that must move,
// moving too little.
function statusOptions(
  from: Picked,
  target: SwapTarget | undefined,
  grounds: Rgb[],
  vision: ColorVision,
  accent: StatusAccent | null,
  marks: Rgb[],
  moveMin: number,
): StatusOption[] {
  const c = from.rgb;
  const seenAccent = accent && seenLab(accent.rgb, vision);
  const accentNeed = accent ? accentNeedOf(accent, c, vision) : 0;
  const option = (rgb: Rgb, h: number): StatusOption => {
    const seen = seenLab(rgb, vision);
    const move = deltaEOk(c, rgb);
    let cost = move * MOVE_COST + (target ? hueOff(h, target.hue) * HUE_COST : 0);
    if (move < moveMin) cost += MOVE_SHORT_COST * (1 + moveMin - move);
    if (seenAccent && accent) {
      const short = accentNeed - seenDistance(seenAccent, seen);
      if (short > 1e-9) cost += (accent.pinned ? PINNED_COST : PICKED_COST) * (1 + short);
    }
    return { rgb, seen, cost };
  };
  // A pin that is not hex stays as it is.
  if (parseHex(from.css) === null) return [option(c, 0)];
  const lch = rgbToOklch(c);
  const floors = grounds.map((g) => Math.min(STATUS_CONTRAST, contrast(c, g)));
  const seenMarks = marks.map((m) => seenLab(m, vision));
  const markFloors = marks.map((m) => [
    Math.min(seenApart(c, m, vision), VISION_GUARD),
    Math.min(deltaEOk(c, m), VISION_GUARD),
  ]);
  const chroma = target ? Math.max(lch.C, target.chroma) : lch.C;
  const keepChroma = target
    ? CHROMA_KEEP * Math.min(lch.C, target.chroma)
    : lch.C > 0.04
      ? HUE_CHROMA_KEEP * lch.C
      : 0;
  const hues: number[] = [];
  if (target) {
    for (let d = -target.reach; d <= target.reach + 1e-9; d += STATUS_HUE_STEP) {
      hues.push((target.hue + d + 360) % 360);
    }
  } else {
    hues.push(lch.h);
  }
  const out: StatusOption[] = [];
  const reach = Math.round(STATUS_REACH / STATUS_STEP);
  for (const h of hues) {
    for (let i = -reach; i <= reach; i += 1) {
      if (!target && i === 0) {
        out.push(option(c, h));
        continue;
      }
      const L = lch.L + i * STATUS_STEP;
      if (L < 0 || L > 1) continue;
      const raw = oklchToRgbInGamut({ L, C: chroma, h });
      const rgb = { r: Math.round(raw.r), g: Math.round(raw.g), b: Math.round(raw.b) };
      if (grounds.some((g, j) => contrast(rgb, g) < floors[j])) continue;
      if (rgbToOklch(rgb).C < keepChroma) continue;
      const o = option(rgb, h);
      const clear = seenMarks.every(
        (m, j) =>
          seenDistance(m, o.seen) >= markFloors[j][0] &&
          deltaEOk(rgb, marks[j]) >= markFloors[j][1],
      );
      if (clear) out.push(o);
    }
  }
  // Where no turned color clears every floor, the Typical color stays.
  return out.length > 0 ? out : [option(c, lch.h)];
}

const STATUS_CACHE = new Map<string, { danger: Picked; warn: Picked; success: Picked }>();

/** The status colors swapped for `vision`. Deuteranopia and protanopia
 *  turn success blue and danger toward vermilion (STATUS_SWAP), each
 *  settling its lightness and its hue inside its window, and warn keeps
 *  its hue and may move in lightness. Tritanopia keeps the Typical colors
 *  where they already stand apart (statusApart), and otherwise moves each
 *  in lightness at its own hue. Floors, firmest first:
 *
 *  1. No color reads fainter on the panel or on raised than the 3:1
 *     floor, or than the Typical color where that sits under it, and
 *     each stands as far from the text the marks sit beside, `marks`, as
 *     the Typical color stands, up to VISION_GUARD, seen and typical. A
 *     turned color keeps CHROMA_KEEP of its chroma, or of the chroma it
 *     aims for if that is less.
 *  2. No color comes nearer a pinned accent than the Typical color
 *     stands, up to ACCENT_APART.
 *  3. Danger comes no nearer warn, seen, than the Typical colors stand,
 *     give or take KEPT_SLACK, and warn and success stay as far apart as
 *     they stand, up to VISION_GUARD.
 *  4. Success moves at least STATUS_MOVE_MIN.
 *  5. No color comes nearer the accent the rule picked for Typical than
 *     the Typical color stands, up to ACCENT_APART. The rule picks again
 *     after, clear of the swapped colors.
 *  6. Danger and success stand STATUS_PART apart, seen, or as far as a
 *     typical eye sees the Typical colors if that is less, give or take
 *     VISION_SLACK, and under tritanopia danger and warn too.
 *
 *  Typical keeps the Typical colors, and so does a color the theme pins
 *  in a form other than hex. */
function statusSeenBy(
  vision: ColorVision,
  typical: { danger: Picked; warn: Picked; success: Picked },
  grounds: Rgb[],
  accent: StatusAccent | null,
  marks: Rgb[],
): { danger: Picked; warn: Picked; success: Picked } {
  if (vision === 'typical') return typical;
  const targets = STATUS_SWAP[vision];
  const turns = Object.keys(targets).length > 0;
  const { danger, warn, success } = typical;
  const key = [
    vision,
    danger.css,
    warn.css,
    success.css,
    ...grounds.map(toHex),
    accent && `${toHex(accent.rgb)} ${accent.pinned}`,
    ...marks.map(toHex),
  ].join(' ');
  const held = STATUS_CACHE.get(key);
  if (held) return held;
  if (!turns && statusApart(vision, typical)) {
    STATUS_CACHE.set(key, typical);
    return typical;
  }
  const options = (k: StatusKey) =>
    statusOptions(
      typical[k],
      targets[k],
      grounds,
      vision,
      accent,
      marks,
      targets[k] && k === 'success' ? STATUS_MOVE_MIN : 0,
    );
  const dangers = options('danger');
  const warns = options('warn');
  const successes = options('success');
  const keepWarn = seenApart(danger.rgb, warn.rgb, vision) - KEPT_SLACK;
  const apart = Math.min(seenApart(warn.rgb, success.rgb, vision), VISION_GUARD);
  const part = Math.min(STATUS_PART, deltaEOk(danger.rgb, success.rgb));
  // Where no color turns, danger stands as far from warn as from success.
  const partWarn = turns ? 0 : Math.min(STATUS_PART, deltaEOk(danger.rgb, warn.rgb));
  let best: {
    cost: number;
    danger: StatusOption;
    warn: StatusOption;
    success: StatusOption;
  } | null = null;
  for (const d of dangers) {
    if (best && d.cost >= best.cost) continue;
    const s = successes.map((o) => {
      const v = seenDistance(d.seen, o.seen);
      let cost = o.cost + Math.max(0, part - v) * PART_SOFT;
      if (v < part - VISION_SLACK) cost += PART_COST * (1 + part - VISION_SLACK - v);
      return { o, cost };
    });
    const w = warns.map((o) => {
      const v = seenDistance(d.seen, o.seen);
      let cost = o.cost + (v < keepWarn ? PAIR_COST * (1 + keepWarn - v) : 0);
      cost += Math.max(0, partWarn - v) * PART_SOFT;
      if (v < partWarn - VISION_SLACK) cost += PART_COST * (1 + partWarn - VISION_SLACK - v);
      return { o, cost };
    });
    const pairCost = (so: (typeof s)[number], wo: (typeof w)[number]) => {
      const v = seenDistance(so.o.seen, wo.o.seen);
      return so.cost + wo.cost + (v < apart ? PAIR_COST * (1 + apart - v) : 0);
    };
    const cheapest = <T extends { cost: number }>(list: T[]) =>
      list.reduce((win, o) => (o.cost < win.cost ? o : win));
    let ss = cheapest(s);
    let ww = cheapest(w);
    let cost = d.cost + pairCost(ss, ww);
    if (seenDistance(ss.o.seen, ww.o.seen) < apart) {
      // The cheapest pair of the two that stands apart, if one does.
      const sSorted = [...s].sort((a, b) => a.cost - b.cost);
      const wSorted = [...w].sort((a, b) => a.cost - b.cost);
      for (const so of sSorted) {
        if (d.cost + so.cost + wSorted[0].cost >= cost) break;
        for (const wo of wSorted) {
          const c = d.cost + pairCost(so, wo);
          if (d.cost + so.cost + wo.cost >= cost) break;
          if (c < cost) {
            cost = c;
            ss = so;
            ww = wo;
          }
        }
      }
    }
    if (!best || cost < best.cost) best = { cost, danger: d, warn: ww.o, success: ss.o };
  }
  const pick = best as NonNullable<typeof best>;
  const keep = (from: Picked, to: StatusOption): Picked =>
    to.rgb === from.rgb ? from : { css: toHex(to.rgb), rgb: to.rgb };
  const out = {
    danger: keep(danger, pick.danger),
    warn: keep(warn, pick.warn),
    success: keep(success, pick.success),
  };
  STATUS_CACHE.set(key, out);
  return out;
}

/** Whether the Typical status colors already stand apart for `vision`,
 *  so a vision that turns none of them keeps them: danger from success
 *  and from warn, as the vision sees them, STATUS_PART or as far as a
 *  typical eye sees them if that is less, give or take VISION_SLACK. The
 *  accent the rule picks then keeps clear of them. */
function statusApart(
  vision: ColorVision,
  status: { danger: Picked; warn: Picked; success: Picked },
): boolean {
  const { danger, warn, success } = status;
  const apart = (a: Rgb, b: Rgb) =>
    seenApart(a, b, vision) >= Math.min(STATUS_PART, deltaEOk(a, b)) - VISION_SLACK;
  return apart(danger.rgb, success.rgb) && apart(danger.rgb, warn.rgb);
}

/** Derive the chrome tokens for a terminal palette, for a player with
 *  `vision`. Pure. Typical derives them as the rule above has it. */
export function deriveChrome(
  x: XtermPalette,
  overrides: ChromeOverrides = {},
  vision: ColorVision = 'typical',
): ChromeTokens {
  const o = overrides;
  const bg = pick(o.bg, hexOr(x.background, FALLBACK_BG));
  const appearance = o.appearance ?? appearanceOf(bg.rgb);
  const dark = appearance === 'dark';
  const dir = dark ? 1 : -1;

  const panel = pick(o.panel, bg.rgb);
  let raisedLight: Rgb = WHITE;
  if (!dark) {
    const paper = rgbToOklch(panel.rgb);
    const lifted = oklchToRgbInGamut({
      L: Math.min(RAISED_LIGHT_MAX_L, paper.L + RAISED_LIGHT_LIFT),
      C: paper.C / 2,
      h: paper.h,
    });
    raisedLight = { r: Math.round(lifted.r), g: Math.round(lifted.g), b: Math.round(lifted.b) };
  }
  const raised = pick(o.raised, dark ? stepOver(panel.rgb, 'raised', true) : raisedLight);
  const grounds = [panel.rgb, raised.rgb];
  const floor = (c: Rgb, target: number) =>
    grounds.reduce((out, ground) => liftAtHue(out, ground, target, dir), c);

  // A foreground too dim for the secondary floor (Solarized Dark) lifts
  // to it so the text tiers stay ordered.
  const text = pick(
    o.text,
    liftAtHue(hexOr(x.foreground, FALLBACK_FG), panel.rgb, SECONDARY_CONTRAST, dir),
  );
  const secondary = pick(
    o.secondary,
    solveAlphaForContrast(text.rgb, panel.rgb, SECONDARY_CONTRAST).color,
  );
  // Tertiary is the foreground over the panel at the lowest alpha that
  // clears 3.1:1 on both grounds.
  let tertiary = text.rgb;
  for (let i = 0; i <= 100; i += 1) {
    const c = composite(text.rgb, panel.rgb, i / 100);
    if (grounds.every((ground) => contrast(c, ground) >= TERTIARY_CONTRAST)) {
      tertiary = c;
      break;
    }
  }

  // Dark themes read the bright ANSI slots, light themes the normal
  // ones, and each moves away from the grounds until it clears 3:1.
  const status = (normal: string, bright: string, fallback: Rgb) =>
    floor(hexOr(dark ? bright : normal, fallback), STATUS_CONTRAST);
  const typicalStatus = {
    danger: pick(o.danger, status(x.red, x.brightRed, { r: 224, g: 108, b: 117 })),
    warn: pick(o.warn, status(x.yellow, x.brightYellow, { r: 229, g: 192, b: 123 })),
    success: pick(o.success, status(x.green, x.brightGreen, { r: 152, g: 195, b: 121 })),
  };
  const liftAccent = (c: Rgb) => liftAtHue(c, panel.rgb, ACCENT_CONTRAST, dir);
  // A color vision tunes the status colors clear of the theme's pinned
  // accent, or of the one the rule picks for Typical, which the rule then
  // keeps where it stands apart.
  let tuned = typicalStatus;
  let prefer: Rgb | undefined;
  if (vision !== 'typical') {
    const t = typicalStatus;
    const pinned = o.accent ? parseHex(o.accent) : null;
    prefer = accentCandidate(x, [t.danger.rgb, t.warn.rgb, t.success.rgb]);
    const accent = o.accent
      ? pinned && { rgb: pinned, pinned: true }
      : { rgb: liftAccent(prefer), pinned: false };
    tuned = statusSeenBy(vision, t, grounds, accent, [text.rgb, secondary.rgb]);
  }
  const { danger, warn, success } = tuned;
  // A danger that clears 3:1 as a dot can still be too dim to read as
  // words (Nord's red sits near 3:1), so text takes its own tier.
  const dangerText = pick(o.dangerText, floor(danger.rgb, STATUS_TEXT_CONTRAST));
  // The same for the warn tone, a yellow that reads as a dot on paper
  // but not as words.
  const warnText = pick(o.warnText, floor(warn.rgb, STATUS_TEXT_CONTRAST));

  const accent = pick(
    o.accent,
    liftAccent(accentCandidate(x, [danger.rgb, warn.rgb, success.rgb], vision, liftAccent, prefer)),
  );

  // Ink for text on an accent fill: the theme's dark end held at or
  // below L 0.24 and darkened further until it reads at 4.5:1, or white
  // when white reads better on the accent.
  const darkEnd = dark ? bg.rgb : text.rgb;
  const darkLab = rgbToOklab(darkEnd);
  const ink =
    darkLab.L > INK_MAX_L ? shiftLightness(darkEnd, INK_MAX_L - darkLab.L) : { ...darkEnd };
  const onAccent = pick(
    o.onAccent,
    contrast(WHITE, accent.rgb) >= contrast(ink, accent.rgb)
      ? WHITE
      : liftAtHue(ink, accent.rgb, ON_ACCENT_CONTRAST, -1),
  );

  // The scheme's own selection and its text when the pair reads, else
  // the accent over the ground with the text tier on it.
  const schemeFill = parseHex(x.selectionBackground);
  const schemeText = parseHex(x.selectionForeground) ?? hexOr(x.foreground, FALLBACK_FG);
  const selection =
    schemeFill !== null &&
    stepDL(schemeFill, bg.rgb) >= SELECTION_STEP &&
    contrast(schemeText, schemeFill) >= SELECTION_TEXT_CONTRAST
      ? { fill: schemeFill, text: schemeText }
      : { fill: composite(accent.rgb, bg.rgb, SELECTION_ALPHA[appearance]), text: text.rgb };

  // The control washes. An override, like pick's, wins as it stands.
  const steps = WASH_STEP[appearance];
  const field = dark ? washOver(panel.rgb, WASH_STEP.dark.field, true) : raised.css;

  return {
    appearance,
    bg: bg.css,
    panel: panel.css,
    sep: pick(o.sep, stepOver(panel.rgb, 'sep', dark)).css,
    divider: pick(o.divider, stepOver(panel.rgb, 'divider', dark)).css,
    selrow: pick(o.selrow, dark ? stepOver(panel.rgb, 'selrow', true) : raised.rgb).css,
    hover: pick(o.hover, stepOver(panel.rgb, 'hover', dark)).css,
    inputband: pick(o.inputband, stepOver(bg.rgb, 'inputband', dark)).css,
    text: text.css,
    secondary: secondary.css,
    tertiary: pick(o.tertiary, tertiary).css,
    title: pick(o.title, secondary.rgb).css,
    raised: raised.css,
    accent: accent.css,
    onAccent: onAccent.css,
    danger: danger.css,
    dangerText: dangerText.css,
    warn: warn.css,
    warnText: warnText.css,
    success: success.css,
    selection: pick(o.selection, selection.fill).css,
    selectionText: pick(o.selectionText, selection.text).css,
    field: o.field || field,
    track: o.track || washOver(panel.rgb, steps.track, dark),
    menuHi: o.menuHi || washOver(raised.rgb, steps.menuHi, dark),
    keyRing: o.keyRing || washOver(panel.rgb, steps.keyRing, dark),
    edge: o.edge || washOver(bg.rgb, steps.edge, dark),
  };
}

/** CSS custom property name for a token (`onAccent` is `--on-accent`). */
export function tokenVarName(key: ChromeColorKey): string {
  return `--${key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;
}

/** The token vars theme.ts writes on :root. */
export function tokensToCssVars(tokens: ChromeTokens): Record<string, string> {
  const out: Record<string, string> = {};
  for (const key of CHROME_COLOR_KEYS) out[tokenVarName(key)] = tokens[key];
  return out;
}
