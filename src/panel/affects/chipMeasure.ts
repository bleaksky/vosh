import { useMemo } from 'react';
import { readPanelFace, readPanelGameFace, usePanelFaceVersion } from '../panelFace';
import { FIXED_MEASURE, type ChipMeasure } from './chipsGrid';
import { PANE_TEXT_PX, paneTextSize, textPx } from '../paneTextSize';

// Text widths for the Grouped chips packer, in the faces the pane draws:
// the game face (--font-panel-game) for the names and hours, and the
// panel face (--font-panel) for the group names and the count, both of
// which follow your Panel font, at your panel size, the group names a
// step smaller, as panel.css draws them. The pane and its minimum share
// this one measure.
//
// A canvas measures a face that has not loaded in its fallback, so the
// widths are dropped and measured again whenever a face finishes
// loading, and whenever a panel face itself changes (panelFace.ts).

let canvas: HTMLCanvasElement | null = null;

/** A measure over the panel faces as they are now, at panel size
 *  `size` px, with its own cache. */
export function liveChipMeasure(size: number = PANE_TEXT_PX): ChipMeasure {
  if (typeof document === 'undefined') return FIXED_MEASURE;
  const px = paneTextSize(size);
  const game = readPanelGameFace();
  const face = readPanelFace();
  const cache = new Map<string, number>();
  const width = (font: string, text: string) => {
    const key = `${font}\u0000${text}`;
    const hit = cache.get(key);
    if (hit !== undefined) return hit;
    canvas ??= document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    if (!ctx) return text.length * 7.2;
    ctx.font = font;
    const w = ctx.measureText(text).width;
    cache.set(key, w);
    return w;
  };
  return {
    mono: (s) => width(`${px}px ${game}`, s),
    hours: (s) => width(`700 ${px}px ${game}`, s),
    label: (s) => width(`600 ${textPx(11, px)}px ${face}`, s),
    count: (s) => width(`${px}px ${face}`, s),
  };
}

/** The live measure at your panel `size`, new each time a face loads
 *  or changes, or the size does, so the pane and its minimum pack
 *  again. */
export function useChipMeasure(size: number = PANE_TEXT_PX): ChipMeasure {
  const v = usePanelFaceVersion();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(() => liveChipMeasure(size), [v, size]);
}
