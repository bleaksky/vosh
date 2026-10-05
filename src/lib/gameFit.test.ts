import { describe, expect, it } from 'vitest';
import { deltaEOk, parseHex, rgbToOklch, toHex } from './color';
import {
  apca,
  checks,
  COLOR_VISIONS,
  fit,
  GAME_FIXED_COLORS,
  GAME_SLOTS,
  holdsCheck,
  holdsVision,
  HUE_LIMIT,
  HUE_TURN,
  HUE_TURN_FAR,
  needsFit,
  needsVisionFit,
  seenApart,
  seenBy,
  TEXT_SLOTS,
  textGuards,
  toColorVision,
  turnHue,
  turnRoom,
  turnSlots,
  visionChecks,
  visionPairs,
  visionSlots,
  xterm256,
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
  const short = (p: XtermPalette) =>
    checks(p)
      .filter((c) => !c.ok)
      .map((c) => `${c.id} ${c.value}`);
  const hue = (hex: string) => rgbToOklch(parseHex(hex) ?? { r: 0, g: 0, b: 0 }).h;

  it('asks the 46 checks the same whatever the vision', () => {
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

  it('keeps red apart from green, bright yellow and yellow, and for a tritanope cyan from green and blue', () => {
    const red = [
      ['red', 'green'],
      ['red', 'brightYellow'],
      ['red', 'yellow'],
    ];
    expect(visionPairs('typical')).toEqual([]);
    expect(visionPairs('deuteranopia')).toEqual(red);
    expect(visionPairs('protanopia')).toEqual(red);
    expect(visionPairs('tritanopia')).toEqual([...red, ['cyan', 'green'], ['cyan', 'blue']]);
  });

  // Triad keeps no fit, so its Typical fit is the published palette. A
  // typical eye sees its red 30.3 from its green, and a deuteranope 10.5.
  it('measures each pair through the vision against what a typical eye sees in the Typical fit', () => {
    expect(visionChecks(TRIAD, TRIAD, 'typical')).toEqual([]);
    expect(visionChecks(TRIAD, TRIAD, 'deuteranopia')).toEqual([
      { id: 'red/green', value: 10.5, need: 30.3, ok: false },
      { id: 'red/brightYellow', value: 24, need: 30.5, ok: false },
      { id: 'red/yellow', value: 14.7, need: 22.6, ok: false },
    ]);
    expect(holdsVision(TRIAD, TRIAD, 'typical')).toBe(true);
    expect(holdsVision(TRIAD, TRIAD, 'deuteranopia')).toBe(false);
    expect(seenApart(parseHex(TRIAD.red)!, parseHex(TRIAD.green)!, 'typical')).toBe(
      deltaEOk(parseHex(TRIAD.red)!, parseHex(TRIAD.green)!),
    );
  });

  it('reads a saved vision and takes anything else as Typical', () => {
    expect(COLOR_VISIONS.map(toColorVision)).toEqual(COLOR_VISIONS);
    expect(toColorVision('deutan')).toBe('typical');
    expect(toColorVision(undefined)).toBe('typical');
  });

  // Kanso Zen's Typical fit already parts every pair as a protanope sees
  // it, so the fit for Protanopia is that fit, the same object.
  it('keeps the Typical fit where it already parts every pair the vision keeps apart', () => {
    const kanso = findTheme('kanso-zen');
    expect(needsVisionFit(kanso.xterm, 'protanopia', kanso.fitted)).toBe(false);
    expect(fit(kanso.xterm, 'protanopia', kanso.fitted)).toBe(kanso.fitted);
    expect(needsVisionFit(kanso.xterm, 'deuteranopia', kanso.fitted)).toBe(true);
    expect(needsVisionFit(TRIAD, 'typical')).toBe(false);
    // A color the fit cannot read.
    expect(needsVisionFit({ ...TRIAD, red: 'crimson' }, 'deuteranopia')).toBe(false);
  });

  // The fit themes.ts ships for Triad under Deuteranopia. Lighter green
  // would run into body text and darker green into its floor, so only red
  // moves, turning 5 degrees toward orange to its limit at 33. Triad
  // still passes all 46 checks.
  it('fits Triad for a deuteranope, lightness first and then a small turn', () => {
    const fitted = fit(TRIAD, 'deuteranopia', {});
    expect(fitted).toEqual({ red: '#fa6346' });
    const play = { ...TRIAD, ...fitted };
    expect(short(play)).toEqual([]);
    expect(visionChecks(play, TRIAD, 'deuteranopia').map((c) => c.value)).toEqual([
      12.1, 25.1, 15.6,
    ]);
    expect(hue(play.red) - hue(TRIAD.red)).toBeCloseTo(5, 0);
    expect(hue(play.red)).toBeCloseTo(HUE_LIMIT.red, 0);
  });

  it('keeps every color a vision moves clear of body text, white and bold white', () => {
    expect(TEXT_SLOTS).toEqual(['foreground', 'white', 'brightWhite']);
    expect(textGuards('typical')).toEqual([]);
    expect(textGuards('deuteranopia')).toHaveLength(6 * 3);
    expect(textGuards('tritanopia')).toHaveLength(10 * 3);
    expect(textGuards('protanopia').slice(0, 3)).toEqual([
      ['red', 'foreground'],
      ['red', 'white'],
      ['red', 'brightWhite'],
    ]);
  });

  it('moves only the colors of the pairs a vision keeps apart and their twins', () => {
    const both = ['red', 'green', 'yellow', 'brightRed', 'brightGreen', 'brightYellow'];
    expect(visionSlots('deuteranopia')).toEqual(both);
    expect(visionSlots('protanopia')).toEqual(both);
    expect(visionSlots('tritanopia')).toEqual([
      'red',
      'green',
      'yellow',
      'blue',
      'cyan',
      'brightRed',
      'brightGreen',
      'brightYellow',
      'brightBlue',
      'brightCyan',
    ]);
    expect(visionSlots('typical')).toEqual([]);
    expect(turnSlots('deuteranopia')).toEqual(['red', 'green']);
    expect(turnSlots('protanopia')).toEqual(['red', 'green']);
    expect(turnSlots('tritanopia')).toEqual(['red', 'green', 'cyan']);
    expect(turnSlots('typical')).toEqual([]);
  });

  it('turns a hue up to the bound and never past its family limit', () => {
    expect([HUE_TURN, HUE_TURN_FAR]).toEqual([30, 40]);
    expect(HUE_LIMIT).toEqual({ red: 33, green: 165, cyan: 240 });
    // Red stops past tomato and short of orange red, green past medium
    // spring green and short of aquamarine, and cyan short of dodger blue.
    expect(hue('#ff6347')).toBeLessThan(HUE_LIMIT.red);
    expect(hue('#ff4500')).toBeGreaterThan(HUE_LIMIT.red);
    expect(hue('#00fa9a')).toBeLessThan(HUE_LIMIT.green);
    expect(hue('#7fffd4')).toBeGreaterThan(HUE_LIMIT.green);
    expect(hue('#1e90ff')).toBeGreaterThan(HUE_LIMIT.cyan);
    expect(turnRoom('red', 0, HUE_TURN)).toBe(30);
    expect(turnRoom('red', 0, HUE_TURN_FAR)).toBe(33);
    expect(turnRoom('red', 29, HUE_TURN_FAR)).toBe(4);
    expect(turnRoom('red', 42, HUE_TURN_FAR)).toBe(0);
    expect(turnRoom('green', 120, HUE_TURN)).toBe(30);
    expect(turnRoom('green', 120, HUE_TURN_FAR)).toBe(40);
    expect(turnRoom('green', 142, HUE_TURN_FAR)).toBe(23);
    expect(turnRoom('green', 172, HUE_TURN)).toBe(0);
    expect(turnRoom('cyan', 200, HUE_TURN_FAR)).toBe(40);
    // A turn of nothing gives the color back.
    expect(turnHue({ r: 254, g: 100, b: 87 }, 0)).toEqual({ r: 254, g: 100, b: 87 });
    expect(hue(toHex(turnHue({ r: 254, g: 100, b: 87 }, 10)))).toBeCloseTo(hue('#fe6457') + 10, 0);
  });

  // A deuteranope sees nothing through the protan or tritan matrices,
  // so a fit for Deuteranopia leaves those pairs to the players who see
  // through them, and holds every other check as the Typical fit has it.
  it('holds every check but the T7 pairs another vision sees through', () => {
    expect(holdsCheck('T3 red Lc', 'deuteranopia')).toBe(true);
    expect(holdsCheck('T6 green pair dE', 'tritanopia')).toBe(true);
    expect(holdsCheck('T7 red/yellow dL', 'protanopia')).toBe(true);
    expect(holdsCheck('T7 deutan red/green', 'deuteranopia')).toBe(true);
    expect(holdsCheck('T7 protan red/green', 'deuteranopia')).toBe(false);
    expect(holdsCheck('T7 tritan cyan/green', 'deuteranopia')).toBe(false);
    expect(holdsCheck('T7 tritan cyan/green', 'tritanopia')).toBe(true);
    expect(holdsCheck('T7 deutan red/green', 'tritanopia')).toBe(false);
    expect(holdsCheck('T7 protan red/green', 'typical')).toBe(true);
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
