import { describe, expect, it } from 'vitest';
import { PANEL_WIDTH_MIN, PANEL_WIDTH_MIN_FRAMELESS } from '../panel/paneLayout';
import { clampSessionsWidth, sessionsColumn, sessionsFold } from './sessionsColumn';

// When the window holds the sessions sidebar: its column, a 320
// terminal and the panel at its floor must fit, or the sidebar folds.

describe('folding the sessions sidebar', () => {
  it('folds in a window 720 wide with the panel open, as frame b8-narrow draws it', () => {
    // 221 for the sidebar, 320 for the terminal and 200 for the panel
    // come to 741, more than 720.
    expect(sessionsColumn(220)).toBe(221);
    expect(sessionsFold(720, 220, PANEL_WIDTH_MIN)).toBe(true);
    expect(sessionsFold(720, 220, PANEL_WIDTH_MIN_FRAMELESS)).toBe(true);
  });

  it('comes back once the window holds all three', () => {
    expect(sessionsFold(741, 220, PANEL_WIDTH_MIN)).toBe(false);
    expect(sessionsFold(740, 220, PANEL_WIDTH_MIN)).toBe(true);
    expect(sessionsFold(789, 220, PANEL_WIDTH_MIN_FRAMELESS)).toBe(false);
  });

  it('keeps the sidebar in a window 720 wide while the panel is hidden', () => {
    expect(sessionsFold(720, 220, 0)).toBe(false);
  });

  it('folds a wider sidebar sooner and a narrower one later', () => {
    expect(sessionsFold(741, 320, PANEL_WIDTH_MIN)).toBe(true);
    expect(sessionsFold(720, 180, PANEL_WIDTH_MIN)).toBe(false);
  });
});

describe('the sessions sidebar width', () => {
  it('holds a drag between 180 and 320', () => {
    expect(clampSessionsWidth(100, 1280, PANEL_WIDTH_MIN)).toBe(180);
    expect(clampSessionsWidth(260.4, 1280, PANEL_WIDTH_MIN)).toBe(260);
    expect(clampSessionsWidth(400, 1280, PANEL_WIDTH_MIN)).toBe(320);
  });

  it('stops short of the width that would fold it', () => {
    // 800 less the line, the terminal and the panel's floor leaves 279.
    expect(clampSessionsWidth(320, 800, PANEL_WIDTH_MIN)).toBe(279);
    expect(sessionsFold(800, 279, PANEL_WIDTH_MIN)).toBe(false);
    expect(clampSessionsWidth(320, 800, 0)).toBe(320);
  });
});
