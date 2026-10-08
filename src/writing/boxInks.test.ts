import { describe, expect, it } from 'vitest';
import { contrast, parseHex } from '../theme/color';
import { BUILTIN_THEMES, findTheme, themeTokens } from '../theme/themes';
import { xtermThemeFor } from '../terminal/terminalTheme';
import { INK_CONTRAST, boxInks, markGround, readableOn } from './boxInks';

const hex = (s: string) => {
  const c = parseHex(s);
  if (!c) throw new Error(`not a color: ${s}`);
  return c;
};

describe('boxInks', () => {
  // Every theme Vosh ships, Vellum among them, with the box on the
  // terminal ground the card draws it on.
  const themes = [...BUILTIN_THEMES, findTheme('vellum')];
  it.each(themes.map((t) => [t.id, t] as const))(
    'every mark and the selection read at 4.5:1 in %s',
    (_id, theme) => {
      const k = themeTokens(theme);
      const ground = xtermThemeFor(theme, true, false, 'typical').background ?? k.bg;
      const inks = boxInks({ ground, ...k });
      const g = hex(ground);
      expect(contrast(hex(inks.warn), markGround(hex(k.warn), g))).toBeGreaterThanOrEqual(
        INK_CONTRAST,
      );
      expect(contrast(hex(inks.danger), markGround(hex(k.danger), g))).toBeGreaterThanOrEqual(
        INK_CONTRAST,
      );
      expect(contrast(hex(inks.selection), hex(k.selection))).toBeGreaterThanOrEqual(INK_CONTRAST);
    },
  );

  it('keeps a pair that already reads', () => {
    const ink = hex('#f2efee');
    expect(readableOn(ink, hex('#201d1c'))).toEqual(ink);
  });

  it('darkens a yellow on paper and lightens it on a dark ground', () => {
    const yellow = hex('#b58900');
    const paper = hex('#fdf6e3');
    const onPaper = readableOn(yellow, paper);
    expect(contrast(onPaper, paper)).toBeGreaterThanOrEqual(INK_CONTRAST);
    expect(onPaper.r).toBeLessThan(yellow.r);
    const night = hex('#3a3000');
    const onNight = readableOn(yellow, night);
    expect(contrast(onNight, night)).toBeGreaterThanOrEqual(INK_CONTRAST);
    expect(onNight.r).toBeGreaterThan(yellow.r);
  });

  it('moves selected text off a fill that sits on its own lightness', () => {
    // Srcery selects in its text color, so the text needs a new tier.
    const fill = hex('#c0c0c0');
    const out = readableOn(fill, fill);
    expect(contrast(out, fill)).toBeGreaterThanOrEqual(INK_CONTRAST);
  });
});
