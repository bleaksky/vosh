import {
  BLACK,
  WHITE,
  composite,
  contrast,
  liftAtHue,
  luminance,
  parseHex,
  toHex,
  type Rgb,
} from '../theme/color';

// The text colors of the marks and the selection in the writing box,
// each held to read at 4.5:1 on the ground it sits on. The marks draw a
// status text on an 18 percent wash of its status color over the box's
// own ground, the terminal's. The selection draws the theme's selected
// text on its selection fill, as the terminal does. A theme whose pair
// falls short, a yellow on paper or a selection fill near the text,
// keeps its hue and moves its lightness until the pair reads.

/** The contrast every ink in the box reaches. */
export const INK_CONTRAST = 4.5;
/** The wash a mark lays under its text. Mirrors the 18 percent in
 *  styles/writing.css. */
export const MARK_WASH = 0.18;

export interface BoxColors {
  /** The box's ground, the terminal's background. */
  ground: string;
  warn: string;
  warnText: string;
  danger: string;
  dangerText: string;
  selection: string;
  selectionText: string;
}

export interface BoxInks {
  /** Text past a width only Vosh keeps, and the other warn marks. */
  warn: string;
  /** Text past a width a help sets. */
  danger: string;
  /** Selected text. */
  selection: string;
}

const rgb = (hex: string, fallback: Rgb): Rgb => parseHex(hex) ?? fallback;

/** `c` moved at its hue until it reads at `target` on `ground`: away
 *  from the ground's lightness first, and the other way when that end
 *  runs out first. */
export function readableOn(c: Rgb, ground: Rgb, target = INK_CONTRAST): Rgb {
  if (contrast(c, ground) >= target) return c;
  const away: 1 | -1 = luminance(ground) < 0.18 ? 1 : -1;
  const first = liftAtHue(c, ground, target, away);
  if (contrast(first, ground) >= target) return first;
  const second = liftAtHue(c, ground, target, away === 1 ? -1 : 1);
  if (contrast(second, ground) >= target) return second;
  // Neither end of the hue reads, so white or black, whichever reads more.
  return contrast(WHITE, ground) >= contrast(BLACK, ground) ? WHITE : BLACK;
}

/** The ground a mark of status color `wash` draws on. */
export function markGround(wash: Rgb, ground: Rgb): Rgb {
  return composite(wash, ground, MARK_WASH);
}

export function boxInks(c: BoxColors): BoxInks {
  const ground = rgb(c.ground, { r: 16, g: 18, b: 24 });
  const text = rgb(c.selectionText, WHITE);
  const mark = (tone: string, ink: string) => {
    const wash = rgb(tone, text);
    return toHex(readableOn(rgb(ink, wash), markGround(wash, ground)));
  };
  return {
    warn: mark(c.warn, c.warnText),
    danger: mark(c.danger, c.dangerText),
    selection: toHex(readableOn(text, rgb(c.selection, ground))),
  };
}
