// What a Settings color field reads and writes (ui/ColorField.tsx). A
// color is saved as lowercase #rrggbb, and null means the theme default.
// The field keeps what you type as a draft and saves it only once it
// reads as a color, so a half typed hex never reaches the terminal. A
// hex only field, for a color the terminal reads as hex, marks any
// other text as an error instead of saving it.

const HEX = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i;

/** A hex color in 3 or 6 digits, with or without the #, as lowercase
 *  #rrggbb. Null for anything else. */
export function normalizeHexColor(text: string): string | null {
  const m = HEX.exec(text.trim());
  if (!m) return null;
  const digits = m[1].toLowerCase();
  const full =
    digits.length === 3
      ? digits
          .split('')
          .map((c) => c + c)
          .join('')
      : digits;
  return `#${full}`;
}

/** What typing `text` into a color field means. `default` for an empty
 *  field, which goes back to the theme default. `color` with the
 *  #rrggbb to save. `draft` while it is not a color yet. With
 *  `final` false (still typing), only six digits count, so typing
 *  #fffc41 does not stop at #ffffcc on the way. */
export function readColorText(
  text: string,
  final: boolean,
): { kind: 'default' } | { kind: 'color'; hex: string } | { kind: 'draft' } {
  const trimmed = text.trim();
  if (trimmed === '') return { kind: 'default' };
  const hex = normalizeHexColor(trimmed);
  if (hex === null) return { kind: 'draft' };
  const digits = trimmed.startsWith('#') ? trimmed.length - 1 : trimmed.length;
  if (!final && digits !== 6) return { kind: 'draft' };
  return { kind: 'color', hex };
}

/** A saved color the terminal can read in a hex only field: six hex
 *  digits, with or without the #. The sent command color and the
 *  divider color read nothing else, so a name, rgb(), a short hex, or
 *  a hex with alpha saved by an older build draws nothing. */
export function isSixDigitHex(value: string): boolean {
  return /^#?[0-9a-f]{6}$/i.test(value.trim());
}

/** What typing `text` into a hex only color field means, like the sent
 *  command color and the divider color. Reads like readColorText, and
 *  adds `invalid` for text that cannot turn into a hex color: a color
 *  name, rgb(), a hex with alpha, or, once you leave the field, a hex
 *  that is not 3 or 6 digits. The field shows it as an error and saves
 *  nothing. */
export function readHexColorText(
  text: string,
  final: boolean,
): { kind: 'default' } | { kind: 'color'; hex: string } | { kind: 'draft' } | { kind: 'invalid' } {
  const trimmed = text.trim();
  if (trimmed === '') return { kind: 'default' };
  const m = /^#?([0-9a-f]*)$/i.exec(trimmed);
  if (!m || m[1].length > 6) return { kind: 'invalid' };
  const read = readColorText(trimmed, final);
  if (read.kind === 'color') return read;
  return final ? { kind: 'invalid' } : { kind: 'draft' };
}

/** The #rrggbb an `<input type=color>` can hold for a saved value, or
 *  null when the value is empty or not a hex color (an older save may
 *  hold any CSS color). */
export function colorInputValue(value: string | null): string | null {
  return value === null ? null : normalizeHexColor(value);
}

/** An `rgb(r, g, b)` or `rgba(...)` string, as a computed style reports
 *  a color, as #rrggbb. Null when it is neither. */
export function rgbStringToHex(value: string): string | null {
  const m = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i.exec(value.trim());
  if (!m) return normalizeHexColor(value);
  const hex = (n: string) => Math.min(255, Number(n)).toString(16).padStart(2, '0');
  return `#${hex(m[1])}${hex(m[2])}${hex(m[3])}`;
}
