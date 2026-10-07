// Where the prompt card sits over the terminal.

import { useCallback, useEffect, useLayoutEffect, useState } from 'react';
import type { PromptShowState } from '../ipc/prompt';
import type { PromptPreviewName } from '../ipc/promptDesign';
import { besideAnchor, cardAnchor, type CardStep } from './cardRules';
import { BAND_OUTSET_Y, dockGap, type CellSize } from './pinnedDock';
import type { CardView, PromptCardHost } from './PromptCard';

/** Where the card sits: from the window's left over your prompt, or
 *  from its right beside the panel. */
export type CardAnchor =
  | { left: number; bottom: number; maxHeight: number }
  | { right: number; bottom: number; maxHeight: number };

/** Place the card over your prompt in `host`, or with `beside` over the
 *  terminal beside the panel, and follow it as the window or the
 *  terminal area resizes and as anything in `after` changes: the step,
 *  a new prompt state (`refresh`), the view, the design or the preview.
 *  Null until the card is first placed. */
export function useCardPlace(
  host: PromptCardHost,
  cell: CellSize | null,
  show: PromptShowState | null,
  beside: boolean,
  after: {
    step: CardStep | null;
    refresh: number;
    view: CardView;
    template: string;
    drawn: PromptPreviewName;
  },
) {
  const { step, refresh, view, template, drawn } = after;
  const [anchor, setAnchor] = useState<CardAnchor | null>(null);

  // Sit over your prompt, and follow it.
  const relayout = useCallback(async () => {
    const area = host.area();
    if (!area) return;
    const rect = area.getBoundingClientRect();
    if (beside) {
      setAnchor(
        besideAnchor({
          areaTop: rect.top,
          areaRight: rect.right,
          areaBottom: rect.bottom,
          viewportW: window.innerWidth,
          viewportH: window.innerHeight,
        }),
      );
      return;
    }
    const cellH = cell?.height ?? 17.5;
    const term = host.terminal();
    let lastRowTop = rect.bottom - cellH;
    let promptTop: number | null = null;
    if (term) {
      const rows = term.getSize().rows;
      lastRowTop = term.rowTop(rows - 1) ?? lastRowTop;
      const region = await term.promptRegion().catch(() => null);
      if (region && region.atBottom) {
        const top = term.rowTop(region.row);
        if (top !== null && top >= rect.top) promptTop = top;
      }
    }
    const pinned = show?.show === 'pinned' && show.capture;
    const dock = pinned ? host.dock() : null;
    const bandRowTop = dock
      ? dock.getBoundingClientRect().top + dockGap(cellH) + BAND_OUTSET_Y
      : null;
    const placed = cardAnchor({
      pinned: Boolean(pinned),
      promptTop,
      lastRowTop,
      bandRowTop,
      cellH,
      areaTop: rect.top,
      viewportH: window.innerHeight,
    });
    setAnchor({ left: rect.left + 12, ...placed });
  }, [host, cell, show, beside]);

  useLayoutEffect(() => {
    void relayout();
  }, [relayout, step, refresh, view, template, drawn]);

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

  return anchor;
}
