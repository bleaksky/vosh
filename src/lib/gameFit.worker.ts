// Runs the game color fit (lib/gameFit) off the main thread, for
// lib/fitOffThread. Each message holds a palette, the color vision to
// fit it for, the Typical fit when there is one, and an id. The answer
// holds the fitted slots under the same id, or null for a palette the
// fit cannot read, such as one with a color that is not hex.

import { fit, toColorVision } from './gameFit';
import type { XtermPalette } from './themes';

interface Ask {
  id: number;
  palette: XtermPalette;
  vision?: string;
  typical?: Partial<XtermPalette>;
}

self.onmessage = (e: MessageEvent<Ask>) => {
  let fitted: Partial<XtermPalette> | null;
  try {
    fitted = fit(e.data.palette, toColorVision(e.data.vision), e.data.typical);
  } catch {
    fitted = null;
  }
  self.postMessage({ id: e.data.id, fitted });
};
