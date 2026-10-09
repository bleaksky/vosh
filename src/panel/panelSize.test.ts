import { describe, expect, it } from 'vitest';
import {
  DEFAULT_PANEL_SIZE,
  normalizePanelSize,
  PANEL_SIZE_TERMINAL,
  resolvePanelSize,
} from './panelSize';

describe('normalizePanelSize', () => {
  it('reads anything but a number as 12, the size the panes drew at', () => {
    for (const value of [undefined, null, '14', true, {}, [], Number.NaN, Infinity]) {
      expect(normalizePanelSize(value)).toBe(DEFAULT_PANEL_SIZE);
    }
    expect(DEFAULT_PANEL_SIZE).toBe(12);
  });

  it('keeps 0, which follows your terminal size', () => {
    expect(normalizePanelSize(0)).toBe(PANEL_SIZE_TERMINAL);
  });

  it('holds a size to 6 to 64 on half steps, as Rust saves it', () => {
    expect(normalizePanelSize(14)).toBe(14);
    expect(normalizePanelSize(13.5)).toBe(13.5);
    expect(normalizePanelSize(13.4)).toBe(13.5);
    expect(normalizePanelSize(13.2)).toBe(13);
    expect(normalizePanelSize(0.2)).toBe(PANEL_SIZE_TERMINAL);
    expect(normalizePanelSize(0.5)).toBe(6);
    expect(normalizePanelSize(3)).toBe(6);
    expect(normalizePanelSize(-2)).toBe(6);
    expect(normalizePanelSize(90)).toBe(64);
  });
});

describe('resolvePanelSize', () => {
  it('draws at your panel size whatever the terminal size', () => {
    expect(resolvePanelSize(12, 14)).toBe(12);
    expect(resolvePanelSize(16, 11)).toBe(16);
    // A config from before the row draws at 12.
    expect(resolvePanelSize(undefined, 18)).toBe(12);
  });

  it('follows the terminal size while the row says Same as terminal', () => {
    expect(resolvePanelSize(PANEL_SIZE_TERMINAL, 14)).toBe(14);
    expect(resolvePanelSize(PANEL_SIZE_TERMINAL, 18)).toBe(18);
    expect(resolvePanelSize(PANEL_SIZE_TERMINAL, Number.NaN)).toBe(12);
  });
});
