import { useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { holdsPage } from './affectsGrid';

// What every Affects pane style shares: the body's measured size, and
// the window a list pages in.

export type Box = { width: number; height: number };

/** The element's size, kept current as it resizes. Null until the
 *  first measure. The width takes in a scroll bar, so it is the width
 *  PanelHost gives the pane and the pane draws the columns its minimum
 *  counts, while the height is what the rows can fill. */
export function useBoxSize(ref: RefObject<HTMLElement | null>): Box | null {
  const [box, setBox] = useState<Box | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const width = el.offsetWidth;
      const height = el.clientHeight;
      setBox((prev) =>
        prev && prev.width === width && prev.height === height ? prev : { width, height },
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return box;
}

/** A window of whole rows that scrolls a page of `pageHeight` px at a
 *  time. `toPage` scrolls a page into view, for the count at the end
 *  of each page. Pointing away and tabbing away scroll back to the
 *  first page, so the affects that matter most show again at a
 *  glance. */
export function usePagedWindow(pageHeight: number) {
  const ref = useRef<HTMLDivElement | null>(null);
  const behavior = (): ScrollBehavior =>
    window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth';
  const home = () => {
    const el = ref.current;
    if (!el || el.scrollTop === 0 || holdsPage(el)) return;
    el.scrollTo({ top: 0, behavior: behavior() });
  };
  const toPage = (page: number) =>
    ref.current?.scrollTo({ top: page * pageHeight, behavior: behavior() });
  return {
    ref,
    toPage,
    onPointerLeave: home,
    onBlur: () => {
      requestAnimationFrame(home);
    },
  };
}
