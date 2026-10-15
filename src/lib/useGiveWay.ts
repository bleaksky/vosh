import { useLayoutEffect, useRef, useState, type RefObject } from 'react';

// Let parts of a drawing go one at a time until it fits, before the
// frame paints, so you never see it run over. The status bars and the
// Stats pane use it. A new width or a new set of items starts it over
// from none.

/** Whether `node` runs past its own edge. */
function runsOver(node: HTMLElement): boolean {
  return node.scrollWidth > node.clientWidth;
}

/** How many of its `steps` a drawing lets go: none while it fits, and
 *  one more for each render `overflows` finds it does not, which by
 *  default is its items running past its edge. `key` names what it
 *  draws, so a new value width, a font or a style starts over. */
export function useGiveWay(
  ref: RefObject<HTMLElement | null>,
  steps: number,
  key: string,
  overflows: (node: HTMLElement) => boolean = runsOver,
): number {
  const [level, setLevel] = useState(0);
  const [width, setWidth] = useState<number | null>(null);
  const seen = useRef<string | null>(null);
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const measure = () => setWidth(node.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [ref]);
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node || width === null) return;
    const now = `${key}|${width}`;
    if (seen.current !== now) {
      seen.current = now;
      if (level !== 0) {
        setLevel(0);
        return;
      }
    }
    if (level < steps && overflows(node)) setLevel(level + 1);
  }, [ref, width, key, level, steps, overflows]);
  return Math.min(level, steps);
}
