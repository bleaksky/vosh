import { describe, expect, it } from 'vitest';
import { STATUS_CONTRAST } from '../../lib/chrome';
import { contrast, parseHex, rgbToOklch } from '../../lib/color';
import { BUILTIN_THEMES, findTheme, themeTokens } from '../../lib/themes';
import { MOON_SLOTS, moonColor, moonSlot } from './moonColors';

const NAMES = ['Lysenties', 'Nercuros', 'Dyphrities'];

describe('moonSlot', () => {
  it('reads each moon from the ANSI slot the game colors its name with', () => {
    expect([...MOON_SLOTS]).toEqual([
      ['lysenties', 'brightWhite'],
      ['nercuros', 'brightCyan'],
      ['dyphrities', 'red'],
    ]);
    expect(moonSlot('Lysenties')).toBe('brightWhite');
    expect(moonSlot('NERCUROS')).toBe('brightCyan');
    expect(moonSlot('dyphrities')).toBe('red');
    expect(moonSlot('Rhon')).toBeNull();
    expect(moonSlot('constructor')).toBeNull();
  });
});

describe('moonColor', () => {
  it('keeps a slot that already clears the floor', () => {
    const nord = findTheme('nord');
    const tokens = themeTokens(nord);
    expect(moonColor('Lysenties', nord.xterm, tokens)).toBe(nord.xterm.brightWhite);
    expect(moonColor('Nercuros', nord.xterm, tokens)).toBe(nord.xterm.brightCyan);
  });

  it('holds 3:1 on the status line ground on every built in theme', () => {
    for (const theme of BUILTIN_THEMES) {
      const tokens = themeTokens(theme);
      const bg = parseHex(tokens.bg)!;
      for (const name of NAMES) {
        const color = moonColor(name, theme.xterm, tokens);
        expect(contrast(parseHex(color)!, bg), `${name} on ${theme.id}`).toBeGreaterThanOrEqual(
          STATUS_CONTRAST,
        );
      }
    }
  });

  it('darkens a pale slot on a light theme and keeps its hue', () => {
    const rubric = findTheme('rubric');
    const tokens = themeTokens(rubric);
    const pale = { ...rubric.xterm, brightCyan: '#9fe0dd' };
    const color = moonColor('Nercuros', pale, tokens);
    expect(color).not.toBe('#9fe0dd');
    const bg = parseHex(tokens.bg)!;
    expect(contrast(parseHex(color)!, bg)).toBeGreaterThanOrEqual(STATUS_CONTRAST);
    const lab = (hex: string) => rgbToOklch(parseHex(hex)!);
    expect(lab(color).L).toBeLessThan(lab('#9fe0dd').L);
    expect(Math.abs(lab(color).h - lab('#9fe0dd').h)).toBeLessThan(10);
  });

  it('draws a moon the table does not know in the tertiary tone', () => {
    const nord = findTheme('nord');
    const tokens = themeTokens(nord);
    expect(moonColor('Rhon', nord.xterm, tokens)).toBe(tokens.tertiary);
    const broken = { ...nord.xterm, red: 'var(--red)' };
    expect(moonColor('Dyphrities', broken, tokens)).toBe(tokens.tertiary);
  });
});
