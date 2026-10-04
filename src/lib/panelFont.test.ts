import { describe, expect, it } from 'vitest';
import { normalizePanelFont, PANEL_FONT_SYSTEM, PANEL_FONT_TERMINAL } from './panelFont';

describe('normalizePanelFont', () => {
  it('reads anything but a string as the terminal font', () => {
    for (const value of [undefined, null, 0, true, {}, []]) {
      expect(normalizePanelFont(value)).toBe(PANEL_FONT_TERMINAL);
    }
    expect(normalizePanelFont('')).toBe('');
    expect(normalizePanelFont('   ')).toBe('');
  });

  it('spells the system font one way, as Rust saves it', () => {
    expect(normalizePanelFont('system')).toBe(PANEL_FONT_SYSTEM);
    expect(normalizePanelFont(' System ')).toBe(PANEL_FONT_SYSTEM);
  });

  it('keeps a font list as written, a family named system included', () => {
    expect(normalizePanelFont(' "Iosevka", Menlo, monospace ')).toBe('"Iosevka", Menlo, monospace');
    expect(normalizePanelFont('"system", Menlo, monospace')).toBe('"system", Menlo, monospace');
  });
});
