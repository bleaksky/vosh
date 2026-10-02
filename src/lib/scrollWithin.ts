// Bringing an element into view inside the one box that scrolls it.
//
// The DOM call for this moves every scrolling ancestor it finds, the
// window frame included. A frame that clips with overflow hidden still
// scrolls from script, so a search hit far down a Help article slid
// the whole Help window up under the traffic lights and left an empty
// band at the bottom. This moves the nearest box that scrolls and only
// that box. It never touches another ancestor or the document.

/** Where the element lands on the vertical axis, as the DOM call has
 *  it. Start puts it at the top, center in the middle, and nearest
 *  moves the least that shows it, or not at all when it shows. */
export type ScrollBlock = 'start' | 'center' | 'nearest';

export interface ScrollWithinOptions {
  block: ScrollBlock;
}

// overlay is the old WebKit spelling of auto.
const SCROLLING = new Set(['auto', 'scroll', 'overlay']);

/** The nearest ancestor of `el` that scrolls: overflow-y auto or
 *  scroll, with content taller than itself. The body and the root
 *  element never count, so the document never moves. */
export function scrollingAncestor(el: Element): Element | null {
  const doc = el.ownerDocument;
  const view = doc.defaultView;
  if (!view) return null;
  for (let node = el.parentElement; node; node = node.parentElement) {
    if (node === doc.body || node === doc.documentElement) return null;
    if (
      SCROLLING.has(view.getComputedStyle(node).overflowY) &&
      node.scrollHeight > node.clientHeight
    ) {
      return node;
    }
  }
  return null;
}

/** Scroll `el` into view the way the DOM call would, along `block`,
 *  but move only its nearest scrolling ancestor. It honors the box's
 *  scroll padding and the element's scroll margin. Across, it moves
 *  the same box the least that shows the element, and only when that
 *  box scrolls across too. A missing element does nothing. */
export function scrollWithin(el: Element | null | undefined, { block }: ScrollWithinOptions): void {
  if (!el) return;
  const box = scrollingAncestor(el);
  const view = el.ownerDocument.defaultView;
  if (!box || !view) return;
  const boxStyle = view.getComputedStyle(box);
  const elStyle = view.getComputedStyle(el);
  const frame = box.getBoundingClientRect();
  const rect = el.getBoundingClientRect();

  // The port is the inside of the box, less its scroll padding.
  const top = frame.top + box.clientTop;
  const down = shift(
    block,
    rect.top - length(elStyle.scrollMarginTop, 0),
    rect.bottom + length(elStyle.scrollMarginBottom, 0),
    top + length(boxStyle.scrollPaddingTop, box.clientHeight),
    top + box.clientHeight - length(boxStyle.scrollPaddingBottom, box.clientHeight),
  );
  move(box, 'scrollTop', down, box.scrollHeight - box.clientHeight);

  if (!SCROLLING.has(boxStyle.overflowX) || box.scrollWidth <= box.clientWidth) return;
  const left = frame.left + box.clientLeft;
  const across = shift(
    'nearest',
    rect.left - length(elStyle.scrollMarginLeft, 0),
    rect.right + length(elStyle.scrollMarginRight, 0),
    left + length(boxStyle.scrollPaddingLeft, box.clientWidth),
    left + box.clientWidth - length(boxStyle.scrollPaddingRight, box.clientWidth),
  );
  move(box, 'scrollLeft', across, box.scrollWidth - box.clientWidth);
}

/** How far the box moves so the span from `lo` to `hi` lands per
 *  `mode` in the port from `portLo` to `portHi`. Nearest follows the
 *  CSSOM View steps. A span inside the port, or one that covers it,
 *  stays. Otherwise the span lines up on its near edge when it is
 *  smaller than the port and on its far edge when it is larger. */
function shift(mode: ScrollBlock, lo: number, hi: number, portLo: number, portHi: number): number {
  if (mode === 'start') return lo - portLo;
  if (mode === 'center') return (lo + hi) / 2 - (portLo + portHi) / 2;
  const size = hi - lo;
  const port = portHi - portLo;
  if (lo >= portLo && hi <= portHi) return 0;
  if (lo < portLo && hi > portHi) return 0;
  if ((lo < portLo && size < port) || (hi > portHi && size > port)) return lo - portLo;
  return hi - portHi;
}

/** Move `box` by `by` along one axis, held between 0 and `max`. A
 *  move that lands where the box already sits writes nothing. */
function move(box: Element, axis: 'scrollTop' | 'scrollLeft', by: number, max: number): void {
  if (by === 0) return;
  const next = Math.max(0, Math.min(box[axis] + by, max));
  if (next !== box[axis]) box[axis] = next;
}

/** A computed length in pixels. A percent counts against `basis`, and
 *  auto or anything that does not read counts as 0. */
function length(value: string, basis: number): number {
  const n = parseFloat(value);
  if (!Number.isFinite(n)) return 0;
  return value.trim().endsWith('%') ? (n / 100) * basis : n;
}
