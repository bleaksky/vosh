import { describe, expect, it, vi } from 'vitest';
import { deltaEOk, parseHex, rgbToOklch } from './color';
import {
  apca,
  CHANNEL_FLOOR,
  CHANNEL_LEAST,
  CHANNEL_PAIRS,
  CHANNEL_SLOTS,
  checks,
  CHROMA_KEEP,
  COLOR_VISIONS,
  CUE_FLOOR,
  CUE_PAIRS,
  CUE_SLOTS,
  familyOf,
  fit,
  GAME_FIXED_COLORS,
  GAME_SLOTS,
  holdsCheck,
  HUE_CHROMA_KEEP,
  KEPT_PAIRS,
  KEPT_SLACK,
  L_REACH,
  largestMove,
  LEAD_SLOTS,
  MISS_SLACK,
  MOVE_MIN,
  MOVE_WANT,
  needsFit,
  PART_MIN,
  PARTED_PAIRS,
  seenApart,
  seenBy,
  SWAP_TARGETS,
  swapFor,
  TEXT_SLOTS,
  toColorVision,
  VISION_GUARD,
  VISION_SLACK,
  xterm256,
} from './gameFit';
import { GAME_CHANNEL_SLOTS } from './gameChannels';
import { BUILTIN_THEMES, findTheme, typicalStart, visionFitOf, type XtermPalette } from './themes';

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
  it('leaves a palette that passes every check as it is', { timeout: 30_000 }, () => {
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

// The swaps pinned to hex: Kanso Zen for a deuteranope from its Typical
// fit, and Solarized Dark for a protanope from its published colors.
const KANSO_DEUTAN: readonly string[] = [
  'red #d67f46',
  'green #83bcfc',
  'blue #918ed9',
  'brightRed #fd9a6d',
  'brightGreen #dfeeff',
  'brightBlue #d2bbff',
];
const SOLARIZED_PROTAN: readonly string[] = [
  'red #dc332f',
  'green #0ebefd',
  'blue #8879d7',
  'magenta #d23581',
  'brightRed #c65100',
  'brightGreen #76d9ff',
  'brightBlue #ae8dea',
];

describe('color vision', () => {
  const hex = (h: string) => parseHex(h) ?? { r: 0, g: 0, b: 0 };
  const moved = (a: string, b: string) => deltaEOk(hex(a), hex(b));

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

  it('reads a saved vision and takes anything else as Typical', () => {
    expect(COLOR_VISIONS.map(toColorVision)).toEqual(COLOR_VISIONS);
    expect(toColorVision('deutan')).toBe('typical');
    expect(toColorVision(undefined)).toBe('typical');
  });

  // Every source the design read lands on the same pairs: blue against
  // orange for red and green deficiencies, and purple for tritanopia.
  it('turns green blue, red toward vermilion and blue violet, or for a tritanope blue purple and magenta pink', () => {
    const red = {
      green: { hue: 240, reach: 15, chroma: 0.11 },
      red: { hue: 45, reach: 15, chroma: 0.13 },
      blue: { hue: 290, reach: 10, chroma: 0.11 },
    };
    expect(SWAP_TARGETS).toEqual({
      typical: {},
      deuteranopia: red,
      protanopia: red,
      tritanopia: {
        blue: { hue: 320, reach: 10, chroma: 0.12 },
        magenta: { hue: 355, reach: 10, chroma: 0.1 },
      },
    });
    // The windows, from one end to the other.
    const window = (t: { hue: number; reach: number }) => [
      (t.hue - t.reach + 360) % 360,
      (t.hue + t.reach) % 360,
    ];
    expect(window(red.green)).toEqual([225, 255]);
    expect(window(red.red)).toEqual([30, 60]);
    expect(window(red.blue)).toEqual([280, 300]);
    expect(window(SWAP_TARGETS.tritanopia.blue ?? red.blue)).toEqual([310, 330]);
    expect(window(SWAP_TARGETS.tritanopia.magenta ?? red.blue)).toEqual([345, 5]);
    expect(CUE_SLOTS.map(familyOf)).toEqual([
      'red',
      'green',
      'yellow',
      'blue',
      'magenta',
      'cyan',
      'red',
      'green',
      'yellow',
      'blue',
      'magenta',
      'cyan',
    ]);
    expect(LEAD_SLOTS).toEqual({
      typical: [],
      deuteranopia: ['green', 'brightGreen'],
      protanopia: ['green', 'brightGreen'],
      tritanopia: ['blue', 'brightBlue'],
    });
  });

  it('holds the floors and targets the design names', () => {
    expect(PART_MIN).toEqual({ typical: 0, deuteranopia: 20, protanopia: 20, tritanopia: 15 });
    expect(MOVE_MIN).toEqual({ lead: 12, bold: 6 });
    expect([MOVE_WANT, MISS_SLACK, L_REACH, CHANNEL_FLOOR, CUE_FLOOR]).toEqual([15, 1, 0.15, 6, 3]);
    expect([VISION_GUARD, VISION_SLACK, KEPT_SLACK, CHANNEL_LEAST]).toEqual([10, 2, 0.5, 4]);
    expect([CHROMA_KEEP, HUE_CHROMA_KEEP]).toEqual([0.75, 0.6]);
    expect(TEXT_SLOTS).toEqual(['foreground', 'white', 'brightWhite']);
    expect(PARTED_PAIRS.deuteranopia).toEqual([
      ['red', 'green'],
      ['brightRed', 'brightGreen'],
      ['green', 'yellow'],
      ['green', 'brightYellow'],
    ]);
    expect(PARTED_PAIRS.protanopia).toEqual(PARTED_PAIRS.deuteranopia);
    expect(PARTED_PAIRS.tritanopia).toEqual([
      ['cyan', 'blue'],
      ['green', 'blue'],
      ['brightCyan', 'brightBlue'],
    ]);
    expect(KEPT_PAIRS.deuteranopia).toEqual([
      ['red', 'yellow'],
      ['red', 'brightYellow'],
      ['blue', 'magenta'],
    ]);
    expect(KEPT_PAIRS.tritanopia).toEqual([
      ['cyan', 'green'],
      ['red', 'yellow'],
      ['red', 'brightYellow'],
      ['red', 'green'],
    ]);
    expect([PARTED_PAIRS.typical, KEPT_PAIRS.typical]).toEqual([[], []]);
    // Every two of the ten channel colors, tells and yells against blue
    // text, and every other two cue colors of a weight.
    expect(CHANNEL_PAIRS).toHaveLength(47);
    expect(CUE_PAIRS).toHaveLength(7);
    const key = ([a, b]: readonly string[]) => [a, b].sort().join('/');
    const channels = new Set(CHANNEL_PAIRS.map(key));
    for (const pair of CUE_PAIRS) {
      expect(channels.has(key(pair)), key(pair)).toBe(false);
      expect(pair[0].startsWith('bright'), key(pair)).toBe(pair[1].startsWith('bright'));
    }
  });

  // The swap reads the channels from the table the chat pane colors
  // them by, so newbie chat in bold green and immortal talk in bold red
  // stand apart from every other channel too, and from hits on you.
  it('keeps every two channels apart, from the table the chat pane reads', () => {
    expect(CHANNEL_SLOTS).toEqual([
      'red',
      'green',
      'yellow',
      'cyan',
      'brightRed',
      'brightGreen',
      'brightYellow',
      'brightBlue',
      'brightMagenta',
      'brightCyan',
    ]);
    for (const slot of GAME_CHANNEL_SLOTS.values()) {
      if (CUE_SLOTS.includes(slot)) expect(CHANNEL_SLOTS, slot).toContain(slot);
    }
    expect(GAME_CHANNEL_SLOTS.get('newbie')).toBe('brightGreen');
    expect(GAME_CHANNEL_SLOTS.get('immortal')).toBe('brightRed');
    const key = ([a, b]: readonly string[]) => [a, b].sort().join('/');
    const pairs = new Set(CHANNEL_PAIRS.map(key));
    for (const pair of [
      ['brightGreen', 'brightBlue'],
      ['brightGreen', 'brightCyan'],
      ['brightGreen', 'brightMagenta'],
      ['brightGreen', 'cyan'],
      ['brightGreen', 'green'],
      ['brightRed', 'yellow'],
      ['brightRed', 'brightYellow'],
      ['green', 'blue'],
      ['cyan', 'blue'],
    ]) {
      expect(pairs.has(key(pair)), key(pair)).toBe(true);
    }
  });

  // A deuteranope sees nothing through the protan or tritan matrices,
  // so a swap for Deuteranopia leaves those pairs to the players who see
  // through them, and holds every other check as its start has it.
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
    expect(seenApart(hex(TRIAD.red), hex(TRIAD.green), 'typical')).toBe(
      deltaEOk(hex(TRIAD.red), hex(TRIAD.green)),
    );
  });

  it('swaps nothing for Typical and hands back the start', () => {
    const kanso = findTheme('kanso-zen');
    expect(swapFor(kanso.xterm, 'typical', kanso.fitted)).toEqual(kanso.fitted);
    expect(swapFor(TRIAD, 'typical')).toEqual({});
    expect(largestMove(TRIAD, TRIAD, GAME_SLOTS)).toBe(0);
    expect(largestMove(TRIAD, { ...TRIAD, red: 'crimson' }, GAME_SLOTS)).toBe(Infinity);
  });

  // The search keeps a step only when it scores better on the tiers, in
  // a fixed order, so the same palette swaps to the same colors every
  // time, here or in the worker, and fit() hands another vision to it.
  it(
    'swaps Tango Dark for a protanope to the same colors every time',
    { timeout: 60_000 },
    async () => {
      const tango = findTheme('tango-dark');
      const once = swapFor(tango.xterm, 'protanopia', tango.fitted);
      expect(swapFor(tango.xterm, 'protanopia', tango.fitted)).toEqual(once);
      expect(fit(tango.xterm, 'protanopia', tango.fitted)).toEqual(once);
      expect(visionFitOf(tango, 'protanopia', true)).toEqual(once);
      // The worker answers the main window with the same slots.
      const answers: unknown[] = [];
      vi.stubGlobal('self', { postMessage: (m: unknown) => answers.push(m) });
      try {
        await import('./gameFit.worker');
        const worker = (globalThis as unknown as { self: { onmessage: (e: unknown) => void } })
          .self;
        worker.onmessage({
          data: { id: 7, palette: tango.xterm, vision: 'protanopia', start: tango.fitted },
        });
      } finally {
        vi.unstubAllGlobals();
      }
      expect(answers).toEqual([{ id: 7, fitted: once }]);
    },
  );

  // Kanso Zen swaps from its Typical fit. Green turns sky blue, red
  // vermilion and blue violet, and bold green, which the fit draws near
  // white, keeps a blue tint.
  it('swaps Kanso Zen for a deuteranope from its Typical fit', { timeout: 30_000 }, () => {
    const kanso = findTheme('kanso-zen');
    const swapped = swapFor(kanso.xterm, 'deuteranopia', kanso.fitted);
    const play = { ...kanso.xterm, ...swapped };
    const typical = { ...kanso.xterm, ...kanso.fitted };
    expect(CUE_SLOTS.filter((k) => play[k] !== typical[k]).map((k) => `${k} ${play[k]}`)).toEqual(
      KANSO_DEUTAN,
    );
    expect(moved(typical.green, play.green)).toBeGreaterThanOrEqual(MOVE_MIN.lead);
    expect(moved(typical.brightGreen, play.brightGreen)).toBeGreaterThanOrEqual(MOVE_MIN.bold);
  });

  // Solarized Dark keeps out of Fit game colors, so its swap starts from
  // the published colors.
  it('swaps Solarized Dark for a protanope from its published colors', { timeout: 30_000 }, () => {
    const dark = findTheme('solarized-dark');
    const swapped = swapFor(dark.xterm, 'protanopia');
    expect(
      CUE_SLOTS.filter((k) => swapped[k] !== undefined).map((k) => `${k} ${swapped[k]}`),
    ).toEqual(SOLARIZED_PROTAN);
    expect(moved(dark.xterm.green, swapped.green ?? '')).toBeGreaterThanOrEqual(MOVE_MIN.lead);
  });

  // Triad passes every check as it stands, and Kanso Zen's Typical fit
  // already kept every pair the old fit kept apart for a protanope. The
  // swap changes both all the same. Triad's bold green as a blue would
  // run into its cabal and clan colors, so newbie chat keeps its green
  // for a deuteranope and a protanope.
  it('swaps a palette whatever it already keeps apart', { timeout: 60_000 }, () => {
    for (const vision of ['deuteranopia', 'protanopia', 'tritanopia'] as const) {
      const [lead, bold] = LEAD_SLOTS[vision];
      const play = { ...TRIAD, ...swapFor(TRIAD, vision) };
      expect(moved(TRIAD[lead], play[lead]), vision).toBeGreaterThanOrEqual(MOVE_MIN.lead);
      if (vision === 'tritanopia') {
        expect(moved(TRIAD[bold], play[bold]), vision).toBeGreaterThanOrEqual(MOVE_MIN.bold);
      } else {
        const hue = (c: string) => rgbToOklch(hex(c)).h;
        expect(Math.abs(hue(play[bold]) - hue(TRIAD[bold])), vision).toBeLessThan(3);
      }
    }
    const kanso = findTheme('kanso-zen');
    const typical = { ...kanso.xterm, ...kanso.fitted };
    const protan = { ...kanso.xterm, ...swapFor(kanso.xterm, 'protanopia', kanso.fitted) };
    expect(moved(typical.green, protan.green)).toBeGreaterThanOrEqual(MOVE_MIN.lead);
    expect(rgbToOklch(hex(protan.green)).h).toBeGreaterThan(225 - 1);
    expect(rgbToOklch(hex(protan.green)).h).toBeLessThan(255 + 1);
  });
});

// Every built in theme ships its fit worked out ahead, in themes.ts,
// and its swap for each other color vision from each start it plays
// (VISION_FITS). After a change to a published palette, to the fit or to
// the swap, work each out again, about two seconds a theme for Typical
// and about two for each swap, with
//
//   VOSH_FIT_THEMES=1 npx vitest run src/theme/gameFit.test.ts
//
// A theme that now fits or swaps to other colors fails and prints the
// block or the row to paste in its place.
describe.runIf(import.meta.env.VOSH_FIT_THEMES)('the fits themes.ts ships', () => {
  const block = (fitted: Partial<XtermPalette>) =>
    ['fitted: {', ...Object.entries(fitted).map(([k, v]) => `  ${k}: '${v}',`), '},'].join('\n');
  const row = (fitted: Partial<XtermPalette>) => GAME_SLOTS.map((k) => fitted[k] ?? '.').join(' ');
  // The fits run for minutes without a break, so each test first lets the
  // runner answer its worker, which gives up on a call left a minute.
  const breathe = () => new Promise((resolve) => setTimeout(resolve, 0));

  for (const theme of BUILTIN_THEMES) {
    it(theme.id, { timeout: 30_000 }, async () => {
      await breathe();
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
      // The start Typical plays with Fit game colors on, the theme's
      // fit, which the test above holds to the fitter, and with it off.
      const starts = Object.keys(typicalStart(theme, true)).length > 0 ? [true, false] : [false];
      for (const on of starts) {
        const start = on ? 'fitted' : 'published';
        it(`${theme.id} ${vision} ${start}`, { timeout: 60_000 }, async () => {
          await breathe();
          const fresh = swapFor(theme.xterm, vision, typicalStart(theme, on));
          const now = visionFitOf(theme, vision, on) ?? {};
          expect(now, `${theme.id} ${vision} ${start} now swaps to '${row(fresh)}'`).toEqual(fresh);
        });
      }
    }
  }
});
