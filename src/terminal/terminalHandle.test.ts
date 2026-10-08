import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));

const themeId = vi.hoisted(() => ({ current: 'obsidian-ember' }));
vi.mock('../theme/theme', () => ({ getCurrentThemeId: () => themeId.current }));

import { searchDecorations } from './terminalHandle';
import { findTheme, themeTokens } from '../theme/themes';

const term = (background?: string) => ({ options: { theme: background ? { background } : {} } });

describe('searchDecorations', () => {
  afterEach(() => {
    themeId.current = 'obsidian-ember';
  });

  it('fills every match in yellow at 28 and the current one at 60', () => {
    expect(searchDecorations(term('#050403'))).toEqual({
      matchBackground: '#403620',
      matchOverviewRuler: '#d8b56a',
      activeMatchBackground: '#846e41',
      activeMatchColorOverviewRuler: '#d8b56a',
    });
  });

  it('follows the theme in front and draws no accent border', () => {
    themeId.current = 'rubric';
    const marks = searchDecorations(term());
    expect(marks.matchBackground).toBe('#c7b795');
    expect(marks.activeMatchBackground).toBe('#988253');
    expect(marks.matchBorder).toBeUndefined();
    expect(marks.activeMatchBorder).toBeUndefined();
    const accent = themeTokens(findTheme('rubric')).accent.toLowerCase();
    for (const value of Object.values(marks)) {
      expect(String(value).toLowerCase()).not.toBe(accent);
    }
  });

  it('lays the marks over the theme ground when the prompt is lifted', () => {
    themeId.current = 'rubric';
    const marks = searchDecorations(term('#f0e5cf00'));
    expect(marks.matchBackground).toBe('#c7b795');
    expect(marks.activeMatchBackground).toBe('#988253');
    expect(marks.matchOverviewRuler).toBe('#5d4000');
  });

  it('takes the theme ground when the terminal ground does not parse', () => {
    themeId.current = 'rubric';
    const marks = searchDecorations(term('rgba(0, 0, 0, 0)'));
    expect(marks.matchBackground).toBe('#c7b795');
    expect(marks.activeMatchBackground).toBe('#988253');
  });
});
