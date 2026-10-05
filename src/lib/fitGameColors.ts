import { useMemo, useSyncExternalStore } from 'react';
import { createStore } from '../stores/store';
import { playPalette, type XtermPalette } from './themes';
import { useActiveTheme } from './useActiveTheme';

// Fit game colors, from UiConfig fit_game_colors. The main window sets
// it when it reads your config and on vosh://fit-game-colors-changed.
// No other window sets it, so Settings and every other window keep the
// published palette.

const store = createStore(false);

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

/** The palette the game draws in while you play, on the theme this
 *  window paints with. */
export function usePlayPalette(): XtermPalette {
  const theme = useActiveTheme();
  const fit = useFitGameColors();
  return useMemo(() => playPalette(theme, fit), [theme, fit]);
}
