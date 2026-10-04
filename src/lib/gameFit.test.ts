import { describe, expect, it } from 'vitest';
import { apca, checks, fit, GAME_FIXED_COLORS, xterm256 } from './gameFit';
import { findTheme, type XtermPalette } from './themes';

// Triad as the Themes review drew it, the one palette that passes every
// check as it stands.
const TRIAD: XtermPalette = {
  background: '#150c22',
  foreground: '#dbdbda',
  cursor: '#44d4e2',
  cursorAccent: '#150c22',
  selectionBackground: '#224458',
  selectionForeground: '#dbdbda',
  black: '#41464d',
  red: '#fe6457',
  green: '#45c6a8',
  yellow: '#eeca71',
  blue: '#78a0d5',
  magenta: '#bb8eba',
  cyan: '#a7c2c4',
  white: '#b5babe',
  brightBlack: '#98a0ab',
  brightRed: '#ff9b8e',
  brightGreen: '#aafddd',
  brightYellow: '#ffeead',
  brightBlue: '#94c2fb',
  brightMagenta: '#dbade1',
  brightCyan: '#84e6ff',
  brightWhite: '#f4f8fb',
};

const passed = (p: XtermPalette) => checks(p).filter((c) => c.ok).length;

describe('apca', () => {
  // The reference values the APCA 0.0.98G-4g readme lists.
  it('matches the published reference pairs', () => {
    expect(apca('#888888', '#ffffff')).toBeCloseTo(63.056469930209424, 10);
    expect(apca('#ffffff', '#888888')).toBeCloseTo(-68.54146436644962, 10);
    expect(apca('#000000', '#aaaaaa')).toBeCloseTo(58.146262578561334, 10);
    expect(apca('#aaaaaa', '#000000')).toBeCloseTo(-56.24113336839742, 10);
    expect(apca('#112233', '#ddeeff')).toBeCloseTo(91.66830811481631, 10);
    expect(apca('#ddeeff', '#112233')).toBeCloseTo(-93.06770049484275, 10);
  });
});

describe('xterm256', () => {
  it('draws the cube and the gray ramp as xterm does', () => {
    expect(xterm256(16)).toBe('#000000');
    expect(xterm256(231)).toBe('#ffffff');
    expect(xterm256(232)).toBe('#080808');
    expect(xterm256(255)).toBe('#eeeeee');
  });

  it('gives the colors the game sends whatever the theme', () => {
    expect(GAME_FIXED_COLORS.map(xterm256)).toEqual([
      '#585858',
      '#b2b2b2',
      '#d7af87',
      '#5fd75f',
      '#00af00',
      '#afaf5f',
      '#626262',
      '#5fafff',
      '#0087ff',
      '#5f5f00',
      '#87d7ff',
      '#ffd700',
      '#ff0000',
      '#eeeeee',
      '#ff87ff',
    ]);
  });
});

describe('checks', () => {
  it('runs 46 checks, T1 to T7', () => {
    expect(checks(TRIAD)).toHaveLength(46);
  });

  it('passes Triad on all 46', () => {
    expect(passed(TRIAD)).toBe(46);
  });

  it('counts the shipped themes as the review did', () => {
    expect(passed(findTheme('kanso-zen').xterm)).toBe(17);
    expect(passed(findTheme('obsidian-ember').xterm)).toBe(30);
  });

  it('names what Kanso Zen misses', () => {
    const misses = checks(findTheme('kanso-zen').xterm).filter((c) => !c.ok);
    expect(misses.find((c) => c.id === 'T4 brightBlack Lc')?.value).toBe(20.1);
    expect(misses.find((c) => c.id === 'T6 fg/brightWhite dL')?.value).toBe(0);
  });
});

describe('fit', () => {
  it('leaves a palette that passes every check as it is', () => {
    expect(fit(TRIAD)).toEqual({});
  });

  // The lifts the Themes review's survey (fit-survey.json) gives Tango
  // Dark. The search draws its steps from a fixed generator, so the same
  // palette fits to the same colors every time.
  it('fits Tango Dark to the survey colors', { timeout: 30_000 }, () => {
    const tango = findTheme('tango-dark').xterm;
    const fitted = fit(tango);
    expect(fitted).toEqual({
      foreground: '#d4d8d0',
      black: '#3d4345',
      red: '#fe4a3b',
      green: '#a3f476',
      yellow: '#dcb834',
      blue: '#70a3e7',
      magenta: '#bc94c2',
      cyan: '#58ccce',
      white: '#d6dad2',
      brightBlack: '#adafab',
      brightRed: '#ff8f82',
      brightGreen: '#caffa6',
      brightBlue: '#98c7f8',
      brightMagenta: '#e4b4df',
      brightCyan: '#4cf2f1',
      brightWhite: '#fbfbf9',
    });
    const short = checks({ ...tango, ...fitted })
      .filter((c) => !c.ok)
      .map((c) => `${c.id} ${c.value}`);
    expect(short).toEqual([
      'T2 brightRed Lc 53.2',
      'T3 red Lc 36.2',
      'T6 green pair dE 7.6',
      'T7 deutan yellow/green 9.5',
    ]);
  });
});
