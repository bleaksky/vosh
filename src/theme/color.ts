// Pure color math for the theme engine. Everything works on sRGB
// triples in 0..255 and converts through OKLab for perceptual edits
// (the chrome derivation shifts lightness at a fixed hue and chroma)
// and through WCAG relative luminance for contrast floors. No DOM, so
// the derivation runs the same in a unit test as in the webview.

/** An sRGB color with channels in 0..255. Channels may be fractional
 *  mid computation; toHex rounds and clamps. */
export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/** OKLab coordinates. L runs 0..1, a and b are roughly -0.4..0.4. */
export interface Oklab {
  L: number;
  a: number;
  b: number;
}

/** OKLCH coordinates. C is chroma, h is the hue angle in degrees. */
export interface Oklch {
  L: number;
  C: number;
  h: number;
}

export const WHITE: Rgb = { r: 255, g: 255, b: 255 };
export const BLACK: Rgb = { r: 0, g: 0, b: 0 };

/** Parse `#rgb` or `#rrggbb` (the leading hash is optional). Anything
 *  else, including rgba() strings and 8 digit hex, returns null. */
export function parseHex(input: string): Rgb | null {
  let h = input.trim();
  if (h.startsWith('#')) h = h.slice(1);
  if (/^[0-9a-f]{3}$/i.test(h)) {
    h = h
      .split('')
      .map((c) => c + c)
      .join('');
  }
  if (!/^[0-9a-f]{6}$/i.test(h)) return null;
  const n = parseInt(h, 16);
  return { r: (n >> 16) & 0xff, g: (n >> 8) & 0xff, b: n & 0xff };
}

function channel(n: number): number {
  return Math.max(0, Math.min(255, Math.round(n)));
}

/** Format as lowercase `#rrggbb`, rounding and clamping each channel. */
export function toHex(c: Rgb): string {
  const h = (n: number) => channel(n).toString(16).padStart(2, '0');
  return `#${h(c.r)}${h(c.g)}${h(c.b)}`;
}

/** Format as `rgba(r, g, b, a)` with rounded channels. */
export function toRgba(c: Rgb, alpha: number): string {
  const a = Math.round(Math.max(0, Math.min(1, alpha)) * 1000) / 1000;
  return `rgba(${channel(c.r)}, ${channel(c.g)}, ${channel(c.b)}, ${a})`;
}

/** A hex or `rgba(r, g, b, a)` color as it paints over an opaque
 *  `ground`, or null for any other text. */
export function paintOver(css: string, ground: Rgb): Rgb | null {
  const m = /^rgba\((\d+),\s*(\d+),\s*(\d+),\s*([\d.]+)\)$/.exec(css);
  if (m) return composite({ r: +m[1], g: +m[2], b: +m[3] }, ground, +m[4]);
  return parseHex(css);
}

/** A 3 or 6 digit hex color as `rgba(r, g, b, a)`. Any other text comes
 *  back unchanged. */
export function hexToRgba(hex: string, alpha: number): string {
  const c = parseHex(hex);
  return c ? toRgba(c, alpha) : hex;
}

/** The color of the first six hex digits, black without them. The
 *  terminal palette reads its colors this way, so an alpha after the
 *  six digits drops away. */
export function hexOrBlack(hex: string): Rgb {
  const m = /^#?([0-9a-f]{6})/i.exec(hex.trim());
  if (!m) return { r: 0, g: 0, b: 0 };
  const n = parseInt(m[1], 16);
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
}

/** xterm's 256 color table: the 16 of `palette`, then the 6x6x6 cube,
 *  then 24 grays. */
export function indexedRgb(n: number, palette: readonly string[]): Rgb {
  if (n < 16) return hexOrBlack(palette[n] ?? '#000000');
  if (n < 232) {
    const i = n - 16;
    const v = (c: number) => (c === 0 ? 0 : 55 + c * 40);
    return { r: v(Math.floor(i / 36)), g: v(Math.floor((i % 36) / 6)), b: v(i % 6) };
  }
  const gray = 8 + (n - 232) * 10;
  return { r: gray, g: gray, b: gray };
}

/** An sRGB channel, 0..255, in linear light, 0..1. */
export function toLinear(v: number): number {
  const c = v / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** A linear light channel, 0..1, back to sRGB 0..255, unrounded and unclamped. */
export function fromLinear(v: number): number {
  const c = v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
  return c * 255;
}

/** The linear sRGB channels of a color, each 0..1. */
export function rgbToLinear(c: Rgb): [number, number, number] {
  return [toLinear(c.r), toLinear(c.g), toLinear(c.b)];
}

export function rgbToOklab(c: Rgb): Oklab {
  return linearToOklab(rgbToLinear(c));
}

/** OKLab from linear sRGB channels, each 0..1. */
export function linearToOklab([r, g, b]: [number, number, number]): Oklab {
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return {
    L: 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    a: 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    b: 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  };
}

/** sRGB from linear channels, each 0..1. */
export function linearToRgb([r, g, b]: [number, number, number]): Rgb {
  return { r: fromLinear(r), g: fromLinear(g), b: fromLinear(b) };
}

// Linear sRGB, 0..1 inside the gamut, with nothing clamped.
function oklabToLinear(c: Oklab): [number, number, number] {
  const l = (c.L + 0.3963377774 * c.a + 0.2158037573 * c.b) ** 3;
  const m = (c.L - 0.1055613458 * c.a - 0.0638541728 * c.b) ** 3;
  const s = (c.L - 0.0894841775 * c.a - 1.291485548 * c.b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

/** Convert back to sRGB. Out of gamut results clamp per channel, which
 *  keeps hue close enough for the small shifts the derivation makes. */
export function oklabToRgb(c: Oklab): Rgb {
  const [r, g, b] = oklabToLinear(c);
  const clamp = (v: number) => Math.max(0, Math.min(255, fromLinear(v)));
  return { r: clamp(r), g: clamp(g), b: clamp(b) };
}

export function rgbToOklch(c: Rgb): Oklch {
  const { L, a, b } = rgbToOklab(c);
  const h = (Math.atan2(b, a) * 180) / Math.PI;
  return { L, C: Math.hypot(a, b), h: h < 0 ? h + 360 : h };
}

function oklchToOklab(c: Oklch): Oklab {
  const rad = (c.h * Math.PI) / 180;
  return { L: c.L, a: c.C * Math.cos(rad), b: c.C * Math.sin(rad) };
}

export function oklchToRgb(c: Oklch): Rgb {
  return oklabToRgb(oklchToOklab(c));
}

// Float slack, so a color that round trips from sRGB counts as inside.
const GAMUT_SLACK = 1e-6;

function inGamut(c: Oklch): boolean {
  return oklabToLinear(oklchToOklab(c)).every((v) => v >= -GAMUT_SLACK && v <= 1 + GAMUT_SLACK);
}

/** Convert to sRGB at the same lightness and hue, giving up only as
 *  much chroma as sRGB needs. A clamp per channel bends the hue of a
 *  color well outside the gamut (a deep yellow turns orange). */
export function oklchToRgbInGamut(c: Oklch): Rgb {
  if (inGamut(c)) return oklchToRgb(c);
  let inside = 0;
  let outside = c.C;
  for (let i = 0; i < 24; i += 1) {
    const mid = (inside + outside) / 2;
    if (inGamut({ ...c, C: mid })) inside = mid;
    else outside = mid;
  }
  return oklchToRgb({ ...c, C: inside });
}

/** Step `c` in OKLCH lightness, away from `ground`, until it reads at
 *  `target` contrast on it: lighter for `dir` 1, darker for -1. The hue
 *  holds at every step, and chroma gives way only where sRGB runs out.
 *  A clamp per channel would turn a deep yellow orange. A color that
 *  already reads at `target` comes back as it is. */
export function liftAtHue(c: Rgb, ground: Rgb, target: number, dir: 1 | -1): Rgb {
  if (contrast(c, ground) >= target) return c;
  const lch = rgbToOklch(c);
  let out = c;
  for (let L = lch.L; L >= 0 && L <= 1; L += dir * 0.005) {
    const raw = oklchToRgbInGamut({ ...lch, L });
    out = { r: Math.round(raw.r), g: Math.round(raw.g), b: Math.round(raw.b) };
    if (contrast(out, ground) >= target) break;
  }
  return out;
}

/** Move a color's OKLab lightness by `delta` at the same a and b (so
 *  the same hue and chroma), then round to a displayable color. */
export function shiftLightness(c: Rgb, delta: number): Rgb {
  const lab = rgbToOklab(c);
  const out = oklabToRgb({ ...lab, L: Math.max(0, Math.min(1, lab.L + delta)) });
  return { r: channel(out.r), g: channel(out.g), b: channel(out.b) };
}

/** WCAG 2 relative luminance. */
export function luminance(c: Rgb): number {
  return 0.2126 * toLinear(c.r) + 0.7152 * toLinear(c.g) + 0.0722 * toLinear(c.b);
}

/** WCAG 2 contrast ratio, 1..21, order independent. */
export function contrast(a: Rgb, b: Rgb): number {
  const la = luminance(a);
  const lb = luminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** Paint `fg` at `alpha` over an opaque `bg` and round to a displayable
 *  color, the way the browser would. */
export function composite(fg: Rgb, bg: Rgb, alpha: number): Rgb {
  const mix = (f: number, g: number) => channel(f * alpha + g * (1 - alpha));
  return { r: mix(fg.r, bg.r), g: mix(fg.g, bg.g), b: mix(fg.b, bg.b) };
}

/** The color `t` of the way from `a` to `b`, unrounded. */
export function mix(a: Rgb, b: Rgb, t: number): Rgb {
  return { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t };
}

/** Each channel times `k`, unrounded. */
export function scaled(c: Rgb, k: number): Rgb {
  return { r: c.r * k, g: c.g * k, b: c.b * k };
}

/** The smallest alpha, in steps of 0.01, at which `fg` composited over
 *  `bg` reaches `target` contrast against `bg`. When even full opacity
 *  falls short the result is `fg` itself at alpha 1. */
export function solveAlphaForContrast(
  fg: Rgb,
  bg: Rgb,
  target: number,
): { alpha: number; color: Rgb } {
  for (let i = 0; i <= 100; i += 1) {
    const color = composite(fg, bg, i / 100);
    if (contrast(color, bg) >= target) return { alpha: i / 100, color };
  }
  return { alpha: 1, color: fg };
}

/** The straight OKLab distance between two colors, times 100. The
 *  chrome rule holds an accent it picks this far from the status colors. */
export function deltaEOk(x: Rgb, y: Rgb): number {
  const p = rgbToOklab(x);
  const q = rgbToOklab(y);
  return 100 * Math.hypot(p.L - q.L, p.a - q.a, p.b - q.b);
}

/** CIELAB (D65) coordinates. */
export interface Lab {
  L: number;
  a: number;
  b: number;
}

export function rgbToLab(c: Rgb): Lab {
  const r = toLinear(c.r);
  const g = toLinear(c.g);
  const b = toLinear(c.b);
  // sRGB to XYZ (D65), normalized to the D65 white.
  const x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
  const y = 0.2126729 * r + 0.7151522 * g + 0.072175 * b;
  const z = (0.0193339 * r + 0.119192 * g + 0.9503041 * b) / 1.08883;
  const f = (t: number) => (t > 216 / 24389 ? Math.cbrt(t) : ((24389 / 27) * t + 16) / 116);
  const fx = f(x);
  const fy = f(y);
  const fz = f(z);
  return { L: 116 * fy - 16, a: 500 * (fx - fy), b: 200 * (fy - fz) };
}

/** CIEDE2000 color difference between two sRGB colors. Around 1 is a
 *  just noticeable difference, and 2 is the tolerance the theme tests
 *  allow. */
export function deltaE2000(x: Rgb, y: Rgb): number {
  return deltaE2000Lab(rgbToLab(x), rgbToLab(y));
}

/** CIEDE2000 on CIELAB coordinates (Sharma, Wu and Dalal 2005). */
export function deltaE2000Lab(p: Lab, q: Lab): number {
  const rad = Math.PI / 180;
  const c1 = Math.hypot(p.a, p.b);
  const c2 = Math.hypot(q.a, q.b);
  const cBar7 = ((c1 + c2) / 2) ** 7;
  const g = 0.5 * (1 - Math.sqrt(cBar7 / (cBar7 + 25 ** 7)));
  const a1 = p.a * (1 + g);
  const a2 = q.a * (1 + g);
  const c1p = Math.hypot(a1, p.b);
  const c2p = Math.hypot(a2, q.b);
  const hue = (b: number, a: number) => {
    if (a === 0 && b === 0) return 0;
    const h = Math.atan2(b, a) / rad;
    return h < 0 ? h + 360 : h;
  };
  const h1 = hue(p.b, a1);
  const h2 = hue(q.b, a2);
  const dL = q.L - p.L;
  const dC = c2p - c1p;
  let dh = 0;
  if (c1p * c2p !== 0) {
    dh = h2 - h1;
    if (dh > 180) dh -= 360;
    else if (dh < -180) dh += 360;
  }
  const dH = 2 * Math.sqrt(c1p * c2p) * Math.sin((dh / 2) * rad);
  const lBar = (p.L + q.L) / 2;
  const cBarP = (c1p + c2p) / 2;
  let hBar = h1 + h2;
  if (c1p * c2p !== 0) {
    if (Math.abs(h1 - h2) <= 180) hBar = (h1 + h2) / 2;
    else hBar = h1 + h2 < 360 ? (h1 + h2 + 360) / 2 : (h1 + h2 - 360) / 2;
  }
  const t =
    1 -
    0.17 * Math.cos((hBar - 30) * rad) +
    0.24 * Math.cos(2 * hBar * rad) +
    0.32 * Math.cos((3 * hBar + 6) * rad) -
    0.2 * Math.cos((4 * hBar - 63) * rad);
  const sl = 1 + (0.015 * (lBar - 50) ** 2) / Math.sqrt(20 + (lBar - 50) ** 2);
  const sc = 1 + 0.045 * cBarP;
  const sh = 1 + 0.015 * cBarP * t;
  const cBarP7 = cBarP ** 7;
  const rt =
    -2 *
    Math.sqrt(cBarP7 / (cBarP7 + 25 ** 7)) *
    Math.sin(60 * Math.exp(-(((hBar - 275) / 25) ** 2)) * rad);
  return Math.sqrt((dL / sl) ** 2 + (dC / sc) ** 2 + (dH / sh) ** 2 + rt * (dC / sc) * (dH / sh));
}
