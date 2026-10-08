import { useCallback, useEffect, useLayoutEffect, useState } from 'react';
import type { PromptCardHost } from '../prompt/PromptCard';
import type { CellSize } from '../prompt/pinnedDock';

// Where the writing card sits over the terminal: 12 px in from the
// terminal's left, its foot 1 px over the sixth row above your prompt,
// so the six newest rows and your prompt stay in view and the game's
// answers to what the card sends land there. In a window too narrow for
// the card it spans the window 12 in from each side, over the panel,
// and its text gets smaller to keep 80 columns. That is the card's own
// place. Once you drag it somewhere else it goes where you put it
// (cardPlace.ts), and Put the card back brings it here.

/** The rows under the card that stay in view, your prompt's aside. */
export const ROWS_IN_VIEW = 6;

/** How close the card's top may come to the terminal's. */
const TOP_MARGIN = 8;

export interface WritingPlace {
  left: number;
  /** Set in a narrow window, where the card spans the window. */
  right: number | null;
  bottom: number;
  maxHeight: number;
  /** The window's size, which a card you moved stays inside. */
  viewW: number;
  viewH: number;
}

/** Where the card's foot goes, from the top of your prompt's row: 1 px
 *  over the sixth row above it. */
export function writingEdge(promptTop: number, cellH: number): number {
  return promptTop - ROWS_IN_VIEW * cellH - 1;
}

/** Place the card over the terminal in `host`, and follow it as the
 *  window or the terminal area resizes. `wide` is the card's own width,
 *  which a narrow window gives up. Null until placed. */
export function useWritingPlace(host: PromptCardHost, cell: CellSize | null, wide: number) {
  const [place, setPlace] = useState<WritingPlace | null>(null);

  const relayout = useCallback(async () => {
    const area = host.area();
    if (!area) return;
    const rect = area.getBoundingClientRect();
    const cellH = cell?.height ?? 17.5;
    const term = host.terminal();
    let promptTop = rect.bottom - cellH;
    if (term) {
      const rows = term.getSize().rows;
      promptTop = term.rowTop(rows - 1) ?? promptTop;
      const region = await term.promptRegion().catch(() => null);
      if (region && region.atBottom) {
        const top = term.rowTop(region.row);
        if (top !== null && top >= rect.top) promptTop = top;
      }
    }
    const edge = writingEdge(promptTop, cellH);
    const narrow = rect.left + 12 + wide > window.innerWidth - 12;
    setPlace({
      left: narrow ? 12 : rect.left + 12,
      right: narrow ? 12 : null,
      bottom: window.innerHeight - edge,
      maxHeight: Math.max(0, edge - (rect.top + TOP_MARGIN)),
      viewW: window.innerWidth,
      viewH: window.innerHeight,
    });
  }, [host, cell, wide]);

  useLayoutEffect(() => {
    void relayout();
  }, [relayout]);

  useEffect(() => {
    const onResize = () => void relayout();
    window.addEventListener('resize', onResize);
    const area = host.area();
    const observer = area ? new ResizeObserver(onResize) : null;
    if (area) observer?.observe(area);
    return () => {
      window.removeEventListener('resize', onResize);
      observer?.disconnect();
    };
  }, [host, relayout]);

  return place;
}

/** The size of `el` as it lays out, following each resize, or null
 *  until it has one. */
export function useBoxSize(el: HTMLElement | null): { w: number; h: number } | null {
  const [size, setSize] = useState<{ w: number; h: number } | null>(null);
  useLayoutEffect(() => {
    if (!el) {
      setSize(null);
      return;
    }
    const measure = () => {
      const w = el.offsetWidth;
      const h = el.offsetHeight;
      setSize((prev) => (prev && prev.w === w && prev.h === h ? prev : { w, h }));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [el]);
  return size;
}
