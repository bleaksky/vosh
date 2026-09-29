// What a Settings number field reads (ui/NumberField.tsx). The field
// holds what you type as a draft and saves a whole number, clamped to
// the setting's bounds, when you press Enter or leave it.

/** Clamp `n` to [min, max] and round it to a whole number. */
export function clampWhole(n: number, min: number, max: number): number {
  return Math.round(Math.min(max, Math.max(min, n)));
}

/** The whole number `text` asks for, clamped to [min, max], or null
 *  when it holds no number. Spaces and a trailing unit someone typed,
 *  like `500 ms`, are fine. */
export function readNumberText(text: string, min: number, max: number): number | null {
  const m = /^\s*(-?\d+(?:\.\d+)?)\s*[a-z]*\s*$/i.exec(text);
  if (!m) return null;
  const n = Number(m[1]);
  return Number.isFinite(n) ? clampWhole(n, min, max) : null;
}
