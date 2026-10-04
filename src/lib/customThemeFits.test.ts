import { afterEach, describe, expect, it, vi } from 'vitest';
import { fitCustomThemes } from './customThemeFits';
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

afterEach(() => {
  setCustomThemes([]);
});

describe('fitCustomThemes', () => {
  it('fits a custom theme that keeps no fit once and plays it in the fit', async () => {
    // Dusk keeps no fit, as Vosh 0.8.1 saves it. Paper kept its own.
    const list = [theme('dusk', '#1a1b26'), theme('paper', '#f7f4ee', { red: '#a8322c' })];
    setCustomThemes(list.map(customToAppTheme));
    const stop = fitCustomThemes();
    const heard = vi.fn();
    const stopHearing = onCustomThemesChanged(heard);

    expect(fitting.asked).toHaveLength(1);
    expect(fitting.asked[0]).toEqual(findTheme('dusk').xterm);
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

    // The list comes again, as a broadcast brings it. Dusk keeps the
    // fit the window holds, and nothing is fitted twice.
    setCustomThemes(list.map(customToAppTheme));
    expect(findTheme('dusk').fitted).toEqual({ red: '#cb7b74', brightBlack: '#94989f' });
    expect(fitting.asked).toHaveLength(1);

    // New colors fit once more.
    setCustomThemes([customToAppTheme(theme('dusk', '#000000'))]);
    expect(findTheme('dusk').fitted).toBeUndefined();
    expect(fitting.asked).toHaveLength(2);
    expect(fitting.asked[1].background).toBe('#000000');

    stopHearing();
    stop();
  });
});
