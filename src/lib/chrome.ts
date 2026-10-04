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
//
// Tertiary and the status colors also draw on raised surfaces (menu
// shortcuts, palette keycaps, a danger row), so their floors hold on
// whichever of the panel and raised reads worse.
//
// A theme can pin any token with an override, and a pin wins as it
// stands. Overridden base colors (bg, panel, raised, text, the status
// colors) feed the tokens derived from them.

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
  type Rgb,
} from './color';
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
  /// Soft hairline between sections inside a pane.
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
 *  ACCENT_APART from every status color, else bright blue. */
function accentCandidate(x: XtermPalette, status: Rgb[]): Rgb {
  const apart = (c: Rgb) => status.every((s) => deltaEOk(c, s) >= ACCENT_APART);
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
  return hues.find(apart) ?? hexOr(x.brightBlue, FALLBACK_BLUE);
}

/** Derive the chrome tokens for a terminal palette. Pure. */
export function deriveChrome(x: XtermPalette, overrides: ChromeOverrides = {}): ChromeTokens {
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
  const danger = pick(o.danger, status(x.red, x.brightRed, { r: 224, g: 108, b: 117 }));
  // A danger that clears 3:1 as a dot can still be too dim to read as
  // words (Nord's red sits near 3:1), so text takes its own tier.
  const dangerText = pick(o.dangerText, floor(danger.rgb, STATUS_TEXT_CONTRAST));
  // The same for the warn tone, a yellow that reads as a dot on paper
  // but not as words.
  const warn = pick(o.warn, status(x.yellow, x.brightYellow, { r: 229, g: 192, b: 123 }));
  const warnText = pick(o.warnText, floor(warn.rgb, STATUS_TEXT_CONTRAST));
  const success = pick(o.success, status(x.green, x.brightGreen, { r: 152, g: 195, b: 121 }));

  const accent = pick(
    o.accent,
    liftAtHue(
      accentCandidate(x, [danger.rgb, warn.rgb, success.rgb]),
      panel.rgb,
      ACCENT_CONTRAST,
      dir,
    ),
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
