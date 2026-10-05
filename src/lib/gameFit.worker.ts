// Runs the game color fit (lib/gameFit) off the main thread, for
// lib/fitOffThread. Each message holds a palette and an id. The answer
// holds the fitted slots under the same id, or null for a palette the
// fit cannot read, such as one with a color that is not hex.

import { fit } from './gameFit';
import type { XtermPalette } from './themes';

self.onmessage = (e: MessageEvent<{ id: number; palette: XtermPalette }>) => {
  let fitted: Partial<XtermPalette> | null;
  try {
    fitted = fit(e.data.palette);
  } catch {
    fitted = null;
  }
  self.postMessage({ id: e.data.id, fitted });
};
