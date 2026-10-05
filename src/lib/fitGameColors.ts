import { useMemo, useSyncExternalStore } from 'react';
import type { ColorVision } from './gameFit';
import { createStore } from './stores/store';
import { getColorVision, setColorVision, subscribeColorVision } from './theme';
import { playPalette, type XtermPalette } from './themes';
import { useActiveTheme } from './useActiveTheme';

// Fit game colors, from UiConfig fit_game_colors, and the color vision
// the game colors swap for, from UiConfig color_vision. The main window
// sets Fit game colors when it reads your config and on
// vosh://fit-game-colors-changed. No other window sets it, so Settings
// and every other window play the published palette for the game
// colors, swapped for a vision other than Typical. The color vision is
// the one the window paints its chrome for (lib/theme), which every
// window sets from its config and on vosh://color-vision-changed.

const store = createStore(false);

export { getColorVision, setColorVision, subscribeColorVision };

export function setFitGameColors(on: boolean): void {
  store.set(on);
}

export function getFitGameColors(): boolean {
  return store.get();
}

export function subscribeFitGameColors(cb: () => void): () => void {
  return store.subscribe(cb);
}

export function useFitGameColors(): boolean {
  return useSyncExternalStore(store.subscribe, store.get, store.get);
}

export function useColorVision(): ColorVision {
  return useSyncExternalStore(subscribeColorVision, getColorVision, getColorVision);
}

/** The palette the game draws in while you play, on the theme this
 *  window paints with, swapped for your color vision. */
export function usePlayPalette(): XtermPalette {
  const theme = useActiveTheme();
  const fit = useFitGameColors();
  const vision = useColorVision();
  return useMemo(() => playPalette(theme, fit, vision), [theme, fit, vision]);
}
