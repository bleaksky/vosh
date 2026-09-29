// Chrome tokens derived from a terminal palette. The window chrome
// (panel, hairlines, text tiers, status colors) follows the terminal
// theme by a fixed rule instead of a hand authored second palette, so
// any imported terminal theme dresses the whole window. The rule is
// otty's, measured for the One Window design (SPEC section 4):
//
//   appearance  OKLab L of the background below 0.6 is dark
//   panel       background shifted in L (+0.04 dark, -0.021 light)
//   hairlines   white or black at low alpha over the panel
//   text tiers  the foreground, then the foreground over the panel at
//               the alpha that reaches 6.0:1 (secondary) and 3.1:1
//               (tertiary)
//   title       white 55% (dark) or black 50% (light) over the ground
//   raised      the ground shifted +0.06 (dark) or white (light)
//   accent      the cursor when it carries color, else bright blue
//   status      ANSI red, yellow, green lifted to 3:1 on the panel
//   danger text danger lifted to 4.5:1, for words drawn in danger,
//               while markers, dots, and meter fills keep danger
//
// A theme can pin any token with an override. Overridden base colors
// (bg, panel, text, accent) feed the tokens derived from them.

import {
  BLACK,
  WHITE,
  composite,
  contrast,
  parseHex,
  rgbToOklab,
  rgbToOklch,
  oklabToRgb,
  shiftLightness,
  solveAlphaForContrast,
  toHex,
  toRgba,
  type Rgb,
} from './color';
import type { XtermPalette } from './themes';

export type Appearance = 'dark' | 'light';

export interface ChromeTokens {
  appearance: Appearance;
  /// Terminal ground. The title band and input band sit on it.
  bg: string;
  /// Right panel ground.
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
  /// Window title in the title band.
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
  warn: string;
  success: string;
  /// Terminal selection, the accent with alpha.
  selection: string;
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
  'success',
  'selection',
] as const satisfies readonly (keyof ChromeTokens)[];

export type ChromeColorKey = (typeof CHROME_COLOR_KEYS)[number];

// The measured constants. Alphas are for white (dark) or black
// (light) composited over the named ground.
const RECIPE = {
  dark: {
    panelShift: 0.04,
    raisedShift: 0.06,
    sep: 0.07,
    divider: 0.05,
    selrow: 0.07,
    hover: 0.045,
    inputband: 0.04,
    title: 0.55,
    selection: 0.22,
  },
  light: {
    panelShift: -0.021,
    // Light rows select and float on plain white.
    raisedShift: 0,
    sep: 0.09,
    divider: 0.07,
    selrow: 0,
    hover: 0.04,
    inputband: 0.035,
    title: 0.5,
    selection: 0.18,
  },
} as const;

export const APPEARANCE_THRESHOLD = 0.6;
export const SECONDARY_CONTRAST = 6.0;
export const TERTIARY_CONTRAST = 3.1;
export const STATUS_CONTRAST = 3.0;
export const STATUS_TEXT_CONTRAST = 4.5;
export const ON_ACCENT_CONTRAST = 4.5;
/// OKLCH chroma above which the cursor counts as a color of its own.
const ACCENT_CHROMA = 0.05;
/// Darkest ink on an accent fill never sits above this OKLab L.
const INK_MAX_L = 0.24;

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

/** Derive the chrome tokens for a terminal palette. Pure. */
export function deriveChrome(x: XtermPalette, overrides: ChromeOverrides = {}): ChromeTokens {
  const o = overrides;
  const bg = pick(o.bg, hexOr(x.background, FALLBACK_BG));
  const appearance = o.appearance ?? appearanceOf(bg.rgb);
  const dark = appearance === 'dark';
  const r = RECIPE[appearance];
  const wash = dark ? WHITE : BLACK;

  const panel = pick(o.panel, shiftLightness(bg.rgb, r.panelShift));
  // A foreground too dim for the secondary floor (One Dark) lifts to it
  // so the text tiers stay ordered.
  const dir = dark ? 1 : -1;
  const text = pick(
    o.text,
    liftToContrast(hexOr(x.foreground, FALLBACK_FG), panel.rgb, SECONDARY_CONTRAST, dir),
  );

  const cursor = parseHex(x.cursor);
  const cursorIsColor =
    cursor !== null &&
    rgbToOklch(cursor).C > ACCENT_CHROMA &&
    toHex(cursor) !== toHex(hexOr(x.foreground, FALLBACK_FG));
  const accent = pick(o.accent, cursorIsColor ? cursor : hexOr(x.brightBlue, FALLBACK_BLUE));

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
      : liftToContrast(ink, accent.rgb, ON_ACCENT_CONTRAST, -1),
  );

  const secondary = solveAlphaForContrast(text.rgb, panel.rgb, SECONDARY_CONTRAST).color;
  const tertiary = solveAlphaForContrast(text.rgb, panel.rgb, TERTIARY_CONTRAST).color;

  // Dark themes read the bright ANSI slots, light themes the normal
  // ones, and each moves away from the panel until it clears 3:1.
  const status = (normal: string, bright: string, fallback: Rgb) =>
    liftToContrast(hexOr(dark ? bright : normal, fallback), panel.rgb, STATUS_CONTRAST, dir);
  const danger = pick(o.danger, status(x.red, x.brightRed, { r: 224, g: 108, b: 117 }));
  // A danger that clears 3:1 as a dot can still be too dim to read as
  // words (Nord's red sits near 3:1), so text takes its own tier.
  const dangerText = pick(
    o.dangerText,
    liftToContrast(danger.rgb, panel.rgb, STATUS_TEXT_CONTRAST, dir),
  );

  return {
    appearance,
    bg: bg.css,
    panel: panel.css,
    sep: pick(o.sep, composite(wash, panel.rgb, r.sep)).css,
    divider: pick(o.divider, composite(wash, panel.rgb, r.divider)).css,
    selrow: pick(o.selrow, dark ? composite(WHITE, panel.rgb, r.selrow) : WHITE).css,
    hover: pick(o.hover, composite(wash, panel.rgb, r.hover)).css,
    inputband: pick(o.inputband, composite(wash, bg.rgb, r.inputband)).css,
    text: text.css,
    secondary: pick(o.secondary, secondary).css,
    tertiary: pick(o.tertiary, tertiary).css,
    title: pick(o.title, composite(wash, bg.rgb, r.title)).css,
    raised: pick(o.raised, dark ? shiftLightness(bg.rgb, r.raisedShift) : WHITE).css,
    accent: accent.css,
    onAccent: onAccent.css,
    danger: danger.css,
    dangerText: dangerText.css,
    warn: pick(o.warn, status(x.yellow, x.brightYellow, { r: 229, g: 192, b: 123 })).css,
    success: pick(o.success, status(x.green, x.brightGreen, { r: 152, g: 195, b: 121 })).css,
    selection: o.selection ?? toRgba(accent.rgb, r.selection),
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
