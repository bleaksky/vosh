import { afterEach, describe, expect, it, vi } from 'vitest';
import { fitThemesInPlay } from './customThemeFits';
import { normalizeUiConfig, type UiConfig } from '../ipc/uiConfig';
import {
  customToAppTheme,
  findTheme,
  onCustomThemesChanged,
  playPalette,
  setCustomThemes,
  type XtermPalette,
} from './themes';

// The fit runs in a worker. Here it answers when the test says, and
// `visions` records the color vision and the start each ask held.
const fitting = vi.hoisted(() => {
  const asked: XtermPalette[] = [];
  const visions: [string, Partial<XtermPalette> | undefined][] = [];
  let answer: (fitted: Partial<XtermPalette> | null) => void = () => {};
  const fitOffThread = vi.fn(
    (palette: XtermPalette, vision = 'typical', start?: Partial<XtermPalette>) =>
      new Promise<Partial<XtermPalette> | null>((resolve) => {
        asked.push(palette);
        visions.push([vision, start]);
        answer = resolve;
      }),
  );
  return {
    asked,
    visions,
    fitOffThread,
    answer: (fitted: Partial<XtermPalette>) => answer(fitted),
  };
});
vi.mock('./fitOffThread', () => ({ fitOffThread: fitting.fitOffThread }));

const theme = (id: string, background: string, fitted?: Record<string, string>) => ({
  id,
  label: id,
  description: '',
  xterm: { background, foreground: '#c0caf5' },
  chrome: {},
  ...(fitted && { fitted }),
});

/** A config that plays `theme`, its custom themes set as the main
 *  window sets them when it loads a config. */
function load(fields: Partial<UiConfig>): UiConfig {
  const base = normalizeUiConfig({
    theme: 'nord',
    auto_update: false,
    font_family: 'Menlo',
    font_size: 14,
    tracked_affects: [],
    enabled_presets: [],
  });
  const cfg = { ...base, ...fields };
  setCustomThemes(cfg.custom_themes.map(customToAppTheme));
  return cfg;
}

afterEach(() => {
  setCustomThemes([]);
  fitting.asked.length = 0;
  fitting.visions.length = 0;
});

describe('fitThemesInPlay', () => {
  it('fits the custom theme in play that keeps no fit once and plays it in the fit', async () => {
    // Dusk keeps no fit, as Vosh 0.8.1 saves it. Paper kept its own,
    // and Ash is not in play.
    const list = [
      theme('dusk', '#1a1b26'),
      theme('paper', '#f7f4ee', { red: '#a8322c' }),
      theme('ash', '#202020'),
    ];
    const cfg = load({ theme: 'dusk', light_theme: 'paper', custom_themes: list });
    fitThemesInPlay(cfg);
    const heard = vi.fn();
    const stopHearing = onCustomThemesChanged(heard);

    expect(fitting.asked).toEqual([findTheme('dusk').xterm]);
    // Play draws Dusk as published until the fit lands.
    expect(playPalette(findTheme('dusk'), true)).toBe(findTheme('dusk').xterm);

    fitting.answer({ red: '#cb7b74', brightBlack: '#94989f' });
    await vi.waitFor(() => expect(findTheme('dusk').fitted).toBeDefined());
    expect(heard).toHaveBeenCalled();
    expect(playPalette(findTheme('dusk'), true)).toEqual({
      ...findTheme('dusk').xterm,
      red: '#cb7b74',
      brightBlack: '#94989f',
    });
    expect(findTheme('paper').fitted).toEqual({ red: '#a8322c' });
    expect(findTheme('ash').fitted).toBeUndefined();

    // The list comes again, as a broadcast from Settings brings it. Dusk
    // keeps the fit the window holds, and nothing is fitted twice.
    setCustomThemes(list.map(customToAppTheme));
    fitThemesInPlay(cfg);
    expect(findTheme('dusk').fitted).toEqual({ red: '#cb7b74', brightBlack: '#94989f' });
    expect(fitting.asked).toHaveLength(1);
    stopHearing();
  });

  it('leaves the lists Settings sends to Settings', () => {
    // A theme you import arrives in a broadcast, which never fits.
    load({ theme: 'nord' });
    setCustomThemes([customToAppTheme(theme('dusk', '#1b1c27'))]);
    expect(fitting.asked).toHaveLength(0);
  });

  it('fits nothing while Fit game colors is off', () => {
    fitThemesInPlay(
      load({ theme: 'dusk', fit_game_colors: false, custom_themes: [theme('dusk', '#1c1d28')] }),
    );
    expect(fitting.asked).toHaveLength(0);
  });

  it('fits the light and dark themes Follow system appearance plays', () => {
    const cfg = load({
      theme: 'nord',
      follow_system_appearance: true,
      light_theme: 'paper',
      dark_theme: 'dusk',
      custom_themes: [theme('paper', '#f6f3ed'), theme('dusk', '#1d1e29')],
    });
    fitThemesInPlay(cfg);
    expect(fitting.asked.map((p) => p.background)).toEqual(['#f6f3ed', '#1d1e29']);
  });
});

describe('swaps for a color vision', () => {
  it('swaps a custom theme in play once for your vision, and plays its Typical fit until then', async () => {
    const list = [theme('dusk', '#1a1b27', { red: '#cb7b74' })];
    load({ theme: 'dusk', custom_themes: list });
    const heard = vi.fn();
    const stopHearing = onCustomThemesChanged(heard);
    const dusk = findTheme('dusk');
    const typical = { ...dusk.xterm, red: '#cb7b74' };

    // Typical plays the fit the theme keeps and asks for nothing.
    expect(playPalette(dusk, true)).toEqual(typical);
    expect(fitting.asked).toHaveLength(0);
    // Deuteranopia plays it too until its swap lands, and asks once,
    // from the Typical fit.
    expect(playPalette(dusk, true, 'deuteranopia')).toEqual(typical);
    expect(playPalette(dusk, true, 'deuteranopia')).toEqual(typical);
    expect(fitting.asked).toEqual([dusk.xterm]);
    expect(fitting.visions).toEqual([['deuteranopia', { red: '#cb7b74' }]]);

    fitting.answer({ red: '#b06a64', green: '#9fe0a0' });
    await vi.waitFor(() => expect(heard).toHaveBeenCalled());
    // The theme comes back as a new object, so a view that keeps it
    // draws again, now in the swap for the vision.
    expect(findTheme('dusk')).not.toBe(dusk);
    expect(playPalette(findTheme('dusk'), true, 'deuteranopia')).toEqual({
      ...dusk.xterm,
      red: '#b06a64',
      green: '#9fe0a0',
    });
    expect(playPalette(findTheme('dusk'), true)).toEqual(typical);

    // The list comes again, as a broadcast from Settings brings it. The
    // window keeps the fit and asks for nothing more.
    setCustomThemes(list.map(customToAppTheme));
    expect(playPalette(findTheme('dusk'), true, 'deuteranopia')).toMatchObject({
      red: '#b06a64',
    });
    expect(fitting.asked).toHaveLength(1);
    stopHearing();
  });

  // The old fit left a theme whose Typical fit kept the pairs apart as
  // it was. The swap changes every theme, so a copy of Kanso Zen asks
  // for one too, from its Typical fit.
  it('swaps a custom theme whatever its Typical fit already keeps apart', () => {
    const kanso = findTheme('kanso-zen');
    const zen = {
      ...theme('zen', kanso.xterm.background, kanso.fitted as Record<string, string>),
      xterm: { ...kanso.xterm },
    };
    load({ theme: 'zen', custom_themes: [zen] });
    const typical = playPalette(findTheme('zen'), true);
    expect(typical).toEqual({ ...kanso.xterm, ...kanso.fitted });
    expect(playPalette(findTheme('zen'), true, 'protanopia')).toEqual(typical);
    expect(fitting.asked).toEqual([kanso.xterm]);
    expect(fitting.visions).toEqual([['protanopia', kanso.fitted]]);
  });

  // With Fit game colors off the game text plays the published colors,
  // and a vision swaps from them.
  it('swaps from the published colors while Fit game colors is off, and asks nothing for a built in theme', async () => {
    load({ theme: 'mist', custom_themes: [theme('mist', '#1b1c28', { red: '#cb7b74' })] });
    const mist = findTheme('mist');
    expect(playPalette(mist, false, 'protanopia')).toBe(mist.xterm);
    expect(fitting.visions).toEqual([['protanopia', {}]]);
    fitting.answer({ green: '#79baf7' });
    await vi.waitFor(() => expect(findTheme('mist')).not.toBe(mist));
    expect(playPalette(findTheme('mist'), false, 'protanopia')).toEqual({
      ...mist.xterm,
      green: '#79baf7',
    });
    // The swap from the Typical fit is another, and Fit game colors on
    // asks for it.
    expect(playPalette(findTheme('mist'), true, 'protanopia')).toEqual({
      ...mist.xterm,
      red: '#cb7b74',
    });
    expect(fitting.visions).toEqual([
      ['protanopia', {}],
      ['protanopia', { red: '#cb7b74' }],
    ]);
    fitting.asked.length = 0;
    playPalette(findTheme('kanso-zen'), true, 'protanopia');
    playPalette(findTheme('kanso-zen'), false, 'protanopia');
    expect(fitting.asked).toHaveLength(0);
  });
});
