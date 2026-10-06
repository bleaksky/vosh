// Runs the game color fit and the color vision swap (theme/gameFit) off
// the main thread, for theme/fitOffThread. Each message holds a palette,
// the color vision to fit it for, the slots a swap starts from when the
// vision is not Typical, and an id. The answer holds the slots under the
// same id, or null for a palette the fit cannot read, such as one with a
// color that is not hex.

import { fit, toColorVision } from './gameFit';
import type { XtermPalette } from './themes';

interface Ask {
  id: number;
  palette: XtermPalette;
  vision?: string;
  start?: Partial<XtermPalette>;
}

self.onmessage = (e: MessageEvent<Ask>) => {
  let fitted: Partial<XtermPalette> | null;
  try {
    fitted = fit(e.data.palette, toColorVision(e.data.vision), e.data.start);
  } catch {
    fitted = null;
  }
  self.postMessage({ id: e.data.id, fitted });
};
