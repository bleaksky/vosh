import { useMemo, useSyncExternalStore } from 'react';
import type { ColorVision } from './gameFit';
import { createStore } from './stores/store';
import { playPalette, type XtermPalette } from './themes';
import { useActiveTheme } from './useActiveTheme';

// Fit game colors, from UiConfig fit_game_colors, and the color vision
// it fits for, from UiConfig color_vision. The main window sets both
// when it reads your config, and on vosh://fit-game-colors-changed and
// vosh://color-vision-changed. No other window sets them, so Settings
// and every other window keep the published palette.

const store = createStore(false);
const visionStore = createStore<ColorVision>('typical');

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

export function setColorVision(vision: ColorVision): void {
  visionStore.set(vision);
}

export function getColorVision(): ColorVision {
  return visionStore.get();
}

export function subscribeColorVision(cb: () => void): () => void {
  return visionStore.subscribe(cb);
}

export function useColorVision(): ColorVision {
  return useSyncExternalStore(visionStore.subscribe, visionStore.get, visionStore.get);
}

/** The palette the game draws in while you play, on the theme this
 *  window paints with, fitted for your color vision. */
export function usePlayPalette(): XtermPalette {
  const theme = useActiveTheme();
  const fit = useFitGameColors();
  const vision = useColorVision();
  return useMemo(() => playPalette(theme, fit, vision), [theme, fit, vision]);
}
