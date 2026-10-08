import { describe, expect, it } from 'vitest';
import type { ColorVision } from '../../theme/gameFit';
import { findTheme, playPalette, themeTokens } from '../../theme/themes';
import { colorChoices, colorMarks, COLOR_MARK_WORDS } from './vitalColorMarks';

// The marks a vital's color list shows, measured on the play palette
// with Fit game colors on, against the window's low and warn tones, as
// your Color vision sees both.

function marks(theme: string, vision: ColorVision, warn: boolean) {
  const t = findTheme(theme);
  const found = colorMarks(playPalette(t, true, vision), themeTokens(t, vision), vision, warn);
  const of = (mark: string) =>
    Object.entries(found)
      .filter(([, m]) => m === mark)
      .map(([slot]) => slot);
  return { low: of('low'), warn: of('warn') };
}

describe('colorMarks', () => {
  it('marks Red low and Green, Yellow and White warn under Kanso Zen, as board 2 draws it', () => {
    expect(marks('kanso-zen', 'typical', true)).toEqual({
      low: ['red'],
      warn: ['green', 'yellow', 'white'],
    });
  });

  it('marks Red low and Yellow, Bright red and Bright cyan warn for a Deuteranope, as board 6 draws it', () => {
    expect(marks('kanso-zen', 'deuteranopia', true)).toEqual({
      low: ['red'],
      warn: ['yellow', 'brightRed', 'brightCyan'],
    });
  });

  it('marks Red low and Yellow warn under Rubric', () => {
    expect(marks('rubric', 'typical', true)).toEqual({ low: ['red'], warn: ['yellow'] });
  });

  it('marks nothing like warn while the warning is off', () => {
    expect(marks('kanso-zen', 'typical', false)).toEqual({ low: ['red'], warn: [] });
  });
});

describe('colorChoices', () => {
  it('lists Default and the sixteen from Mana in bright blue, as board 2 draws the list', () => {
    const t = findTheme('kanso-zen');
    const palette = playPalette(t, true, 'typical');
    const found = colorMarks(palette, themeTokens(t, 'typical'), 'typical', true);
    const rows = colorChoices(palette, found, 12).map(
      (c) =>
        `${c.label}${c.mark ? ` ${COLOR_MARK_WORDS[c.mark]}` : ''}${c.checked ? ' checked' : ''}`,
    );
    expect(rows).toEqual([
      'Default',
      'Black',
      'Red Like low',
      'Green Like warn',
      'Yellow Like warn',
      'Blue',
      'Magenta',
      'Cyan',
      'White Like warn',
      'Bright black',
      'Bright red',
      'Bright green',
      'Bright yellow',
      'Bright blue checked',
      'Bright magenta',
      'Bright cyan',
      'Bright white',
    ]);
    const [fallback, , red] = colorChoices(palette, found, undefined);
    expect(fallback).toMatchObject({ slot: null, swatch: null, checked: true });
    expect(red).toMatchObject({ slot: 1, swatch: palette.red, checked: false });
  });
});
