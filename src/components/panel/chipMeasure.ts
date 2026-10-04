import { useMemo } from 'react';
import { readPanelFace, usePanelFaceVersion } from '../../lib/panelFace';
import { FIXED_MEASURE, type ChipMeasure } from './chipsGrid';
import { PANE_TEXT_PX, paneTextSize } from './paneTextSize';

// Text widths for the Grouped chips packer, in the face the pane draws:
// the panel face (--font-panel, which follows your Panel font) at your
// terminal size for names and hours, and at the pane's own sizes for
// the group names and the count. The pane and its minimum share this
// one measure.
//
// A canvas measures a face that has not loaded in its fallback, so the
// widths are dropped and measured again whenever a face finishes
// loading, and whenever the panel face itself changes (panelFace.ts).

let canvas: HTMLCanvasElement | null = null;

/** A measure over the panel face as it is now, game text at `size` px,
 *  with its own cache. */
export function liveChipMeasure(size: number = PANE_TEXT_PX): ChipMeasure {
  if (typeof document === 'undefined') return FIXED_MEASURE;
  const px = paneTextSize(size);
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
    mono: (s) => width(`${px}px ${face}`, s),
    hours: (s) => width(`700 ${px}px ${face}`, s),
    label: (s) => width(`600 11px ${face}`, s),
    count: (s) => width(`12px ${face}`, s),
  };
}

/** The live measure at your terminal `size`, new each time a face
 *  loads or changes, or the size does, so the pane and its minimum
 *  pack again. */
export function useChipMeasure(size: number = PANE_TEXT_PX): ChipMeasure {
  const v = usePanelFaceVersion();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(() => liveChipMeasure(size), [v, size]);
}
