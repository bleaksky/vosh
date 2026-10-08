import { useEffect, useMemo, useState } from 'react';

// The prompt card sets terminal text, a prompt line or a preset's sample,
// in the terminal's face at 13 px on 17.5 px rows, and places each
// character on its own cell so a mark lines up with it.

/** The card's terminal text size and row height. */
export const CARD_MONO_PX = 13;
export const CARD_ROW_PX = 17.5;

/** What measuring needs of a 2D canvas context. */
interface Measurer {
  font: string;
  measureText: (text: string) => { width: number };
}

/** The width of one cell of `family` at the card's size. 7.8 is
 *  JetBrains Mono's, where nothing can measure. */
export function measureCell(family: string, context: () => Measurer | null): number {
  try {
    const ctx = context();
    if (!ctx) return 7.8;
    ctx.font = `${CARD_MONO_PX}px ${family}`;
    const width = ctx.measureText('0000000000').width / 10;
    return width > 0 ? width : 7.8;
  } catch {
    return 7.8;
  }
}

/** The part of a FontFaceSet the hook listens to. */
type FontLoads = EventTarget & { ready: Promise<unknown> };

/** Call `cb` when the page's fonts finish loading, and each time a face
 *  loads later. Returns the unsubscribe. */
export function subscribeFontLoads(fonts: FontLoads, cb: () => void): () => void {
  let alive = true;
  const heard = () => {
    if (alive) cb();
  };
  void fonts.ready.then(heard, () => {});
  fonts.addEventListener('loadingdone', heard);
  return () => {
    alive = false;
    fonts.removeEventListener('loadingdone', heard);
  };
}

/** The width of one cell of `family` at the card's size. A face still
 *  loading measures as its fallback, so it measures again once the fonts
 *  load. */
export function useCellWidth(family: string): number {
  const [loads, setLoads] = useState(0);
  useEffect(() => {
    if (typeof document === 'undefined' || !document.fonts) return;
    return subscribeFontLoads(document.fonts, () => setLoads((n) => n + 1));
  }, []);
  return useMemo(
    () => measureCell(family, () => document.createElement('canvas').getContext('2d')),
    // loads marks a face that finished loading.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [family, loads],
  );
}

/** The width of `text` in the UI face at `px`, for placing the names
 *  under a prompt's values. */
export function useLabelMeasure(px: number): (text: string) => number {
  return useMemo(() => {
    let ctx: CanvasRenderingContext2D | null = null;
    try {
      ctx = document.createElement('canvas').getContext('2d');
      if (ctx) {
        const family =
          getComputedStyle(document.documentElement).getPropertyValue('--font-ui').trim() ||
          'system-ui';
        ctx.font = `${px}px ${family}`;
      }
    } catch {
      ctx = null;
    }
    return (text: string) => (ctx ? ctx.measureText(text).width : text.length * px * 0.55);
  }, [px]);
}
