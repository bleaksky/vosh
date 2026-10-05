import { describe, expect, it } from 'vitest';
import {
  apca,
  checks,
  COLOR_VISIONS,
  fit,
  GAME_FIXED_COLORS,
  GAME_SLOTS,
  holdsVision,
  needsFit,
  seenBy,
  toColorVision,
  visionSlots,
  xterm256,
  type ColorVision,
} from './gameFit';
import { BUILTIN_THEMES, findTheme, visionFitOf, type XtermPalette } from './themes';

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

  it('asks for a fit only where one can move a color', () => {
    expect(needsFit(TRIAD)).toBe(false);
    expect(needsFit(findTheme('tango-dark').xterm)).toBe(true);
    // A color the fit cannot read.
    expect(needsFit({ ...findTheme('tango-dark').xterm, red: 'crimson' })).toBe(false);
  });

  // The lifts the Themes review's survey gave Tango Dark, the fit
  // themes.ts ships for it. The search draws its steps from a fixed
  // generator, so the same palette fits to the same colors every time.
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

describe('color vision', () => {
  const need = (vision: ColorVision, id: string) =>
    checks(TRIAD, vision).find((c) => c.id === id)?.need;
  const short = (p: XtermPalette, vision: ColorVision) =>
    checks(p, vision)
      .filter((c) => !c.ok)
      .map((c) => `${c.id} ${c.value}`);

  it('asks Typical the 46 checks as they stood', () => {
    for (const theme of BUILTIN_THEMES) {
      expect(checks(theme.xterm, 'typical'), theme.id).toEqual(checks(theme.xterm));
    }
    expect(
      checks(TRIAD)
        .filter((c) => /^T7 (protan|deutan|tritan) /.test(c.id))
        .map((c) => c.need),
    ).toEqual([
      '>=12',
      '>=12',
      '>=10',
      '>=10',
      '>=10',
      '>=12',
      '>=12',
      '>=10',
      '>=10',
      '>=10',
      '>=8',
    ]);
  });

  it('raises a quarter the floors of the pairs each vision sees through, and no others', () => {
    for (const vision of COLOR_VISIONS) expect(checks(TRIAD, vision)).toHaveLength(46);
    expect(need('deuteranopia', 'T7 deutan red/yellow')).toBe('>=15');
    expect(need('deuteranopia', 'T7 deutan red/brightYellow')).toBe('>=15');
    expect(need('deuteranopia', 'T7 deutan red/green')).toBe('>=12.5');
    expect(need('deuteranopia', 'T7 deutan yellow/green')).toBe('>=12.5');
    expect(need('deuteranopia', 'T7 protan red/green')).toBe('>=10');
    expect(need('deuteranopia', 'T7 tritan cyan/green')).toBe('>=8');
    expect(need('protanopia', 'T7 protan brightRed/brightGreen')).toBe('>=12.5');
    expect(need('protanopia', 'T7 deutan red/yellow')).toBe('>=12');
    expect(need('tritanopia', 'T7 tritan cyan/green')).toBe('>=10');
    expect(need('tritanopia', 'T7 deutan red/green')).toBe('>=10');
    // The lightness and body text floors stay as Typical asks them.
    expect(need('deuteranopia', 'T7 red/yellow dL')).toBe('>=10');
    expect(need('deuteranopia', 'T3 red Lc')).toBe('>=45');
  });

  it('reads a saved vision and takes anything else as Typical', () => {
    expect(COLOR_VISIONS.map(toColorVision)).toEqual(COLOR_VISIONS);
    expect(toColorVision('deutan')).toBe('typical');
    expect(toColorVision(undefined)).toBe('typical');
  });

  it('says whether a palette holds the floors a vision raises', () => {
    // Triad keeps red dE 14.7 from yellow and 10.5 from green for a
    // deuteranope, past the floors Typical asks and short of the ones
    // Deuteranopia asks.
    expect(short(TRIAD, 'typical')).toEqual([]);
    expect(short(TRIAD, 'deuteranopia')).toEqual([
      'T7 deutan red/yellow 14.7',
      'T7 deutan red/green 10.5',
    ]);
    expect(holdsVision(TRIAD, 'typical')).toBe(true);
    expect(holdsVision(TRIAD, 'deuteranopia')).toBe(false);
    expect(holdsVision(TRIAD, 'tritanopia')).toBe(true);
  });

  it('keeps the Typical fit where it already holds the vision', () => {
    // Triad as published holds the tritanopia floor, so it stays as it is.
    expect(fit(TRIAD, 'tritanopia', {})).toEqual({});
    expect(needsFit(TRIAD, 'tritanopia')).toBe(false);
    expect(needsFit(TRIAD, 'deuteranopia')).toBe(true);
  });

  // The fit themes.ts ships for Triad under Deuteranopia. It moves red a
  // touch darker and green, yellow and bright yellow lighter, each at its
  // own hue, keeps every floor Typical asks, and parts every pair
  // Deuteranopia raises.
  it('fits Triad for a deuteranope by lightness alone', { timeout: 30_000 }, () => {
    const fitted = fit(TRIAD, 'deuteranopia', {});
    expect(fitted).toEqual({
      red: '#fb6154',
      green: '#52d1b3',
      yellow: '#f8d47a',
      brightYellow: '#fff3c7',
    });
    const play = { ...TRIAD, ...fitted };
    expect(short(play, 'typical')).toEqual([]);
    expect(short(play, 'deuteranopia')).toEqual([]);
  });

  it('moves only the colors of the pairs a vision raises and their twins', () => {
    const both = ['red', 'green', 'yellow', 'brightRed', 'brightGreen', 'brightYellow'];
    expect(visionSlots('deuteranopia')).toEqual(both);
    expect(visionSlots('protanopia')).toEqual(both);
    expect(visionSlots('tritanopia')).toEqual(['green', 'cyan', 'brightGreen', 'brightCyan']);
    expect(visionSlots('typical')).toEqual([]);
  });

  // Iceberg Dark keeps yellow at Lc 58.1 and red at Lc 37.8 in its
  // Typical fit. Its fit for a deuteranope parts red from green further
  // and leaves both no fainter.
  it('keeps every check the Typical fit passes and lets none it misses fall further', () => {
    const iceberg = findTheme('iceberg-dark');
    const typical = iceberg.fitted ?? {};
    const fitted = fit(iceberg.xterm, 'deuteranopia', typical);
    const play = { ...iceberg.xterm, ...fitted };
    expect(short(play, 'typical')).toEqual(short({ ...iceberg.xterm, ...typical }, 'typical'));
    expect(short(play, 'typical')).toEqual(['T2 yellow Lc 58.1', 'T3 red Lc 37.8']);
    for (const slot of GAME_SLOTS.filter((k) => !visionSlots('deuteranopia').includes(k))) {
      expect(play[slot], slot).toBe({ ...iceberg.xterm, ...typical }[slot]);
    }
  });

  // Harbor Dark under Typical keeps red at Lc 40.8, short of 45. The
  // search finds no step that parts its raised pairs further without
  // darkening red or giving up a check the Typical fit passes, so a
  // deuteranope plays the Typical fit.
  it('keeps the Typical fit where every step parts a pair only by a trade', () => {
    const harbor = findTheme('harbor-dark');
    const typical = harbor.fitted ?? {};
    expect(fit(harbor.xterm, 'deuteranopia', typical)).toBe(typical);
  });

  it('shows a color as the vision sees it, through the matrices the checks use', () => {
    expect(seenBy('#fe6457', 'typical')).toBe('#fe6457');
    expect(seenBy('rgba(255, 255, 255, 0.1)', 'deuteranopia')).toBe('rgba(255, 255, 255, 0.1)');
    for (const vision of COLOR_VISIONS) {
      expect(seenBy('#000000', vision), vision).toBe('#000000');
      expect(seenBy('#ffffff', vision), vision).toBe('#ffffff');
    }
    // Triad's scarlet turns olive for a deuteranope and a protanope, as
    // board 9 of the Themes review draws it.
    expect(seenBy('#fe6457', 'deuteranopia')).toBe('#b3a353');
    expect(seenBy('#fe6457', 'protanopia')).toBe('#8d8255');
    expect(seenBy('#fe6457', 'tritanopia')).toBe('#ff4262');
  });
});

// Every built in theme ships its fit worked out ahead, in themes.ts,
// for Typical and for each other color vision (VISION_FITS). After a
// change to a published palette or to the fit, fit each one again,
// about two seconds a theme for Typical and about one for each other
// vision, with
//
//   VOSH_FIT_THEMES=1 npx vitest run src/lib/gameFit.test.ts
//
// A theme that now fits to other colors fails and prints the block or
// the row to paste in its place.
describe.runIf(import.meta.env.VOSH_FIT_THEMES)('the fits themes.ts ships', () => {
  const block = (fitted: Partial<XtermPalette>) =>
    ['fitted: {', ...Object.entries(fitted).map(([k, v]) => `  ${k}: '${v}',`), '},'].join('\n');
  const row = (fitted: Partial<XtermPalette>) => GAME_SLOTS.map((k) => fitted[k] ?? '.').join(' ');

  for (const theme of BUILTIN_THEMES) {
    it(theme.id, { timeout: 30_000 }, () => {
      // Solarized Dark keeps out of the fit (Q20), so it ships none.
      if (theme.fitGameColors === false) {
        expect(theme.fitted).toBeUndefined();
        return;
      }
      const fresh = fit(theme.xterm);
      // A theme that passes every check fits to no change and ships none.
      const want = Object.keys(fresh).length > 0 ? fresh : undefined;
      expect(theme.fitted, `${theme.id} now fits to\n${block(fresh)}`).toEqual(want);
    });
    for (const vision of COLOR_VISIONS.filter((v) => v !== 'typical')) {
      it(`${theme.id} ${vision}`, { timeout: 60_000 }, () => {
        if (theme.fitGameColors === false) return;
        // The Typical fit themes.ts ships, which the test above holds to
        // the fitter, so each vision reads it as fit() would.
        const fresh = fit(theme.xterm, vision, theme.fitted ?? {});
        const now = visionFitOf(theme, vision) ?? {};
        const kept = fresh === (theme.fitted ?? {}) ? 'the Typical fit' : `'${row(fresh)}'`;
        expect(now, `${theme.id} ${vision} now fits to ${kept}`).toEqual(fresh);
      });
    }
  }
});
