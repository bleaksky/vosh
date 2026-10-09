// Text sizes on half steps. The terminal size, the panel size and the
// command line size each take a half step, such as 13.5, the way Rust
// stores them (src-tauri/src/profile/text_size.rs). A font that scales
// takes any of them. A bitmap only font draws cleanly only at the sizes
// it holds, its strikes, so a size for it keeps to whole sizes, or to
// its strikes when Vosh can read them (font_sizing in
// src-tauri/src/app/system_fonts.rs).

/** The smallest size Vosh saves. */
export const MIN_TEXT_SIZE = 6;
/** The largest size Vosh saves. */
export const MAX_TEXT_SIZE = 64;

/** What sizes a font draws at. */
export interface FontSizing {
  /** True for a font that scales, and for one Vosh cannot judge. */
  half_sizes: boolean;
  /** The pixel sizes a bitmap only font holds, smallest first. Empty
   *  when the font scales or lists none. */
  strikes: number[];
}

/** A font that takes any size, half steps among them. */
export const SCALABLE: FontSizing = { half_sizes: true, strikes: [] };

/** `n` on the nearest half step. Halves round up, as Rust rounds them. */
export function roundHalf(n: number): number {
  return Math.round(n * 2) / 2;
}

/** A saved size as the page reads it: on the nearest half step and held
 *  to 6 to 64, with 0 kept when `zero` names it, the way the panel size
 *  and the command line size follow the terminal. `fallback` for
 *  anything but a number. */
export function normalizeTextSize(value: unknown, fallback: number, zero = false): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
  const size = roundHalf(value);
  if (zero && size === 0) return 0;
  return Math.min(MAX_TEXT_SIZE, Math.max(MIN_TEXT_SIZE, size));
}

/** The strikes of `sizing` that Vosh can save, 6 to 64. */
function usableStrikes(sizing: FontSizing): number[] {
  return sizing.strikes.filter((s) => s >= MIN_TEXT_SIZE && s <= MAX_TEXT_SIZE);
}

/** `size` on a size the font draws at: itself for a font that scales,
 *  else the nearest of its strikes, or the nearest whole size when it
 *  lists none. 0, which follows the terminal, stays 0. */
export function snapTextSize(size: number, sizing: FontSizing): number {
  if (size === 0 || sizing.half_sizes) return size;
  const strikes = usableStrikes(sizing);
  if (strikes.length === 0) return Math.round(size);
  return strikes.reduce((best, s) => (Math.abs(s - size) < Math.abs(best - size) ? s : best));
}

/** The sizes a Size select offers for a font, before your own size:
 *  every half step from 11 to 18 for a font that scales, the strikes of
 *  a bitmap only font, or the whole sizes when it lists none. */
export function offeredTextSizes(sizing: FontSizing): number[] {
  if (sizing.half_sizes) {
    return Array.from({ length: 15 }, (_, i) => 11 + i / 2);
  }
  const strikes = usableStrikes(sizing);
  return strikes.length > 0 ? strikes : [11, 12, 13, 14, 15, 16, 18];
}

/** The line under a Size row that says a font keeps to some sizes, or
 *  empty for a font that takes half sizes. */
export function textSizeNote(sizing: FontSizing): string {
  if (sizing.half_sizes) return '';
  const strikes = usableStrikes(sizing);
  if (strikes.length === 0) return 'This font comes in whole sizes only.';
  const named = strikes.map(String);
  const list =
    named.length === 1 ? named[0] : `${named.slice(0, -1).join(', ')} and ${named.at(-1)}`;
  return `This font comes in ${list} pt only.`;
}
