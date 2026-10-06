import { describe, expect, it } from 'vitest';
import { resolveThemeTerminalColors } from './themes';

describe('resolveThemeTerminalColors', () => {
  it('tints output with the theme by default', () => {
    expect(resolveThemeTerminalColors(null)).toBe(true);
  });

  it('lets the stored choice win', () => {
    expect(resolveThemeTerminalColors(false)).toBe(false);
    expect(resolveThemeTerminalColors(true)).toBe(true);
  });
});
