// The game's backtick color codes as the writing card draws them: the
// color a code writes, from `color_table` (ansi.h:156) through
// `process_color` (comm.c:1957). Each entry of the table is one of eight
// colors, plain or bold, with blink and underline in later blocks that
// the card draws as their color. A code at a line's start colors the
// line in the card as it does for a looker.

/** Each code and the entry of `color_table` it writes. */
const CODES: Record<string, number> = {
  '1': 1,
  '2': 2,
  '3': 3,
  '4': 4,
  '5': 5,
  '6': 6,
  '7': 7,
  '8': 16,
  '9': 13,
  '0': 12,
  '!': 9,
  '@': 10,
  '#': 11,
  $: 12,
  ')': 12,
  '%': 13,
  '^': 14,
  '&': 15,
  '*': 16,
  q: 17,
  w: 18,
  e: 19,
  r: 20,
  t: 21,
  y: 22,
  u: 23,
  i: 32,
  Q: 25,
  W: 26,
  E: 27,
  R: 28,
  T: 29,
  Y: 30,
  U: 31,
  I: 32,
  a: 33,
  s: 34,
  d: 35,
  f: 36,
  g: 37,
  h: 38,
  j: 39,
  k: 48,
  A: 41,
  S: 42,
  D: 43,
  F: 44,
  G: 45,
  H: 46,
  J: 47,
  K: 48,
  z: 49,
  x: 50,
  c: 51,
  v: 52,
  b: 53,
  n: 54,
  m: 55,
  ',': 64,
  Z: 57,
  X: 58,
  C: 59,
  V: 60,
  B: 61,
  N: 62,
  M: 63,
  '<': 64,
};

const NAMES = ['black', 'red', 'green', 'yellow', 'blue', 'purple', 'cyan', 'white'];

/** The terminal color a code writes, 0 to 15, and whether it is bold,
 *  or null for a code that writes no color, the reset among them. The
 *  table runs in blocks of eight, red to white then black, each block
 *  plain or bold in turn (ansi.h:156). */
export function codeSlot(code: string): { color: number; bold: boolean; name: string } | null {
  const entry = CODES[code];
  if (entry === undefined) return null;
  const block = Math.floor((entry - 1) / 8);
  const at = (entry - 1) % 8;
  const base = at === 7 ? 0 : at + 1;
  const bold = block % 2 === 1;
  return { color: bold ? base + 8 : base, bold, name: `${bold ? 'bold ' : ''}${NAMES[base]}` };
}
