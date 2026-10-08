// The High Contrast pair (Board 14). Every text tone reads 7:1 or better
// on every ground the window draws it on, the accent and the lines 3:1,
// and every game color 7:1 on the terminal ground.

import { describe, expect, it } from 'vitest';
import type { ChromeColorKey, ChromeTokens } from './chrome';
import { contrast, paintOver, parseHex, type Rgb } from './color';
import { COLOR_VISIONS, GAME_SLOTS } from './gameFit';
import { findTheme, themeTokens } from './themes';

/** A color as it paints, a translucent one over `ground`. */
const rgb = (color: string, ground?: Rgb): Rgb => {
  const c = ground ? paintOver(color, ground) : parseHex(color);
  if (!c) throw new Error(`not a color ${color}`);
  return c;
};

/** The grounds text sits on: the window, menus and dialogs, the input
 *  band, a selected row, and the hovered row of a menu. */
const grounds = (t: ChromeTokens): Record<string, Rgb> => ({
  bg: rgb(t.bg),
  raised: rgb(t.raised),
  inputband: rgb(t.inputband),
  selrow: rgb(t.selrow),
  menuHi: rgb(t.menuHi, rgb(t.raised)),
});

const TEXT_TONES = [
  'text',
  'secondary',
  'tertiary',
  'title',
  'dangerText',
  'warnText',
  'success',
] as const satisfies readonly ChromeColorKey[];

// The washes the board drew, from boardsR21/out/hc-main-d.json and
// hc-main-l.json.
const BOARD: Record<string, Partial<ChromeTokens>> = {
  'high-contrast': {
    inputband: '#101010',
    selrow: '#1a1a1a',
    hover: '#131313',
    menuHi: 'rgba(255, 255, 255, 0.082)',
    field: 'rgba(255, 255, 255, 0.071)',
    divider: '#181818',
  },
  'high-contrast-light': {
    inputband: '#f6f6f6',
    selrow: '#ffffff',
    hover: '#f1f1f1',
    menuHi: 'rgba(0, 0, 0, 0.049)',
    field: '#ffffff',
    divider: '#eaeaea',
  },
};

describe.each(Object.keys(BOARD))('%s', (id) => {
  const theme = findTheme(id);
  const t = themeTokens(theme);

  it('derives the washes the board drew', () => {
    expect(theme.id).toBe(id);
    expect(t).toMatchObject(BOARD[id]);
  });

  it('reads every text tone 7:1 or better on every ground', () => {
    for (const vision of COLOR_VISIONS) {
      const v = themeTokens(theme, vision);
      for (const [name, ground] of Object.entries(grounds(v))) {
        for (const key of TEXT_TONES) {
          expect(
            contrast(rgb(v[key]), ground),
            `${vision} ${key} on ${name}`,
          ).toBeGreaterThanOrEqual(7);
        }
        expect(
          contrast(rgb(v.accent), ground),
          `${vision} accent on ${name}`,
        ).toBeGreaterThanOrEqual(3);
      }
    }
  });

  it('reads text 7:1 on the selection and on the accent', () => {
    expect(contrast(rgb(t.selectionText), rgb(t.selection))).toBeGreaterThanOrEqual(7);
    expect(contrast(rgb(t.onAccent), rgb(t.accent))).toBeGreaterThanOrEqual(7);
  });

  it('draws lines and field edges 3:1 on the window and on menus', () => {
    for (const key of ['sep', 'edge', 'keyRing'] as const) {
      for (const ground of ['bg', 'raised'] as const) {
        expect(contrast(rgb(t[key]), rgb(t[ground])), `${key} on ${ground}`).toBeGreaterThanOrEqual(
          3,
        );
      }
    }
  });

  it('reads every game color 7:1 on the terminal ground', () => {
    const ground = rgb(theme.xterm.background);
    for (const slot of GAME_SLOTS) {
      expect(contrast(rgb(theme.xterm[slot]), ground), slot).toBeGreaterThanOrEqual(7);
    }
  });
});

it('plays black as #9a9a9a on the dark ground', () => {
  expect(findTheme('high-contrast').xterm.black).toBe('#9a9a9a');
});

// Two light slots sit just over the line by the WCAG formula.
it('clears 7:1 with the two light slots nearest it', () => {
  const white = rgb('#ffffff');
  expect(contrast(rgb('#884900'), white)).toBeGreaterThanOrEqual(7);
  expect(contrast(rgb('#595959'), white)).toBeGreaterThanOrEqual(7);
});
