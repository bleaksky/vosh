import { useCallback, useEffect, useRef, useState } from 'react';

// When the card beside a session's row opens and closes. It opens once
// the pointer has rested on a row for half a second, and at once on the
// next row while it stays open, as macOS tooltips do. It closes when
// the pointer leaves the rows, on a click, on any key and while a row
// is in the air, and a row a click or a key closed it on stays quiet
// until the pointer leaves that row.

/** How long the pointer rests on a row before its card opens. */
export const CARD_DELAY = 500;

/** The card under way in the sidebar. */
export interface HoverCard {
  /** The session whose card shows, and the slot it sits level with. */
  shown: { session: number; slot: HTMLElement } | null;
  /** The pointer moved on `session`'s slot. */
  rest: (session: number, slot: HTMLElement) => void;
  /** The pointer left the rows, or they moved under it. */
  leave: () => void;
}

/** When the card opens and closes, while `lifted` says whether a row is
 *  in the air. */
export function useHoverCard(lifted: boolean): HoverCard {
  const [shown, setShown] = useState<HoverCard['shown']>(null);
  const open = useRef<HoverCard['shown']>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // The session under the pointer, and the one a click or a key closed
  // the card on.
  const under = useRef<number | null>(null);
  const quiet = useRef<number | null>(null);

  const show = useCallback((next: HoverCard['shown']) => {
    clearTimeout(timer.current);
    timer.current = undefined;
    open.current = next;
    setShown(next);
  }, []);

  const rest = (session: number, slot: HTMLElement) => {
    if (session !== under.current) quiet.current = null;
    under.current = session;
    if (lifted || quiet.current === session) return;
    if (open.current) {
      if (open.current.session !== session) show({ session, slot });
      return;
    }
    clearTimeout(timer.current);
    timer.current = setTimeout(() => show({ session, slot }), CARD_DELAY);
  };

  const leave = () => {
    under.current = null;
    quiet.current = null;
    show(null);
  };

  useEffect(() => {
    if (lifted) show(null);
  }, [lifted, show]);

  // A click or a key anywhere closes it, and keeps it shut on that row.
  useEffect(() => {
    const dismiss = () => {
      quiet.current = under.current;
      show(null);
    };
    window.addEventListener('pointerdown', dismiss, true);
    window.addEventListener('keydown', dismiss, true);
    return () => {
      window.removeEventListener('pointerdown', dismiss, true);
      window.removeEventListener('keydown', dismiss, true);
      show(null);
    };
  }, [show]);

  return { shown, rest, leave };
}
