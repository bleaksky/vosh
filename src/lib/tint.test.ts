import { describe, expect, it } from 'vitest';
import { resolveThemeTerminalColors } from './session';

describe('resolveThemeTerminalColors', () => {
  it('tints output with the theme by default for every theme', () => {
    for (const theme of ['obsidian-ember', 'nord', 'vellum', 'custom-2']) {
      expect(resolveThemeTerminalColors(theme, null)).toBe(true);
    }
  });

  it('lets the stored choice win', () => {
    expect(resolveThemeTerminalColors('obsidian-ember', false)).toBe(false);
    expect(resolveThemeTerminalColors('nord', true)).toBe(true);
  });
});
