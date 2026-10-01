import { useMemo } from 'react';

// The prompt card sets terminal text, a prompt line or a preset's sample,
// in the terminal's face at 13 px on 17.5 px rows, as the boards do, and
// places each character on its own cell so a mark lines up with it.

/** The card's terminal text size and row height. */
export const CARD_MONO_PX = 13;
export const CARD_ROW_PX = 17.5;

/** The width of one cell of `family` at the card's size, measured once
 *  per family. 7.8 is JetBrains Mono's, where nothing can measure. */
export function useCellWidth(family: string): number {
  return useMemo(() => {
    try {
      const canvas = document.createElement('canvas');
      const ctx = canvas.getContext('2d');
      if (!ctx) return 7.8;
      ctx.font = `${CARD_MONO_PX}px ${family}`;
      const width = ctx.measureText('0000000000').width / 10;
      return width > 0 ? width : 7.8;
    } catch {
      return 7.8;
    }
  }, [family]);
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
