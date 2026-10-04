import { afterEach, describe, expect, it, vi } from 'vitest';
import { fitThemesInPlay } from './customThemeFits';
import { normalizeUiConfig, type UiConfig } from './session';
import {
  customToAppTheme,
  findTheme,
  onCustomThemesChanged,
  playPalette,
  setCustomThemes,
  type XtermPalette,
} from './themes';

// The fit runs in a worker. Here it answers when the test says.
const fitting = vi.hoisted(() => {
  const asked: XtermPalette[] = [];
  let answer: (fitted: Partial<XtermPalette> | null) => void = () => {};
  const fitOffThread = vi.fn(
    (palette: XtermPalette) =>
      new Promise<Partial<XtermPalette> | null>((resolve) => {
        asked.push(palette);
        answer = resolve;
      }),
  );
  return { asked, fitOffThread, answer: (fitted: Partial<XtermPalette>) => answer(fitted) };
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
