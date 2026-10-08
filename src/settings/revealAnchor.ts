import { showCoach } from '../ui/coach';

// Bringing a deep link or search hit into view. The frame hands this
// the anchors a target names (settingsScrollIds), best first. Pages
// draw some rows only after their data loads, and content above an
// anchor can grow after it shows, so this keeps looking for a while,
// settles for the section when the row never shows, and holds the
// anchor in place until the page stops moving or you scroll yourself.
// An anchor that carries data-st-coach is something to pick, as Show me
// in Get started points at Add affect. It takes focus and the coach
// ring with that line in place of the flash.

const FLASH_MS = 1200;
const FIND_EVERY_MS = 50;
// Look for 2 s in all.
const FIND_TRIES = 40;
// Wait 300 ms for the row before taking the section around it.
const SECTION_AFTER_TRIES = 6;
// Hold the anchor in place this long while the page lays out.
const HOLD_MS = 1500;
const HOLD_EVERY_MS = 100;

const flashTimers = new WeakMap<HTMLElement, number>();

/** The element that scrolls `el`: the nearest scrolling ancestor
 *  inside `root`, or `root`. A page that scrolls inside itself (the
 *  Automation list) has its own. */
function scrollerFor(el: HTMLElement, root: HTMLElement): HTMLElement {
  for (let node = el.parentElement; node && node !== root; node = node.parentElement) {
    const { overflowY } = getComputedStyle(node);
    if ((overflowY === 'auto' || overflowY === 'scroll') && node.scrollHeight > node.clientHeight) {
      return node;
    }
  }
  return root;
}

/** Where `scroller` should sit to show `el`: a section at the top
 *  under the scroll padding, a row in the middle. */
function scrollTopFor(el: HTMLElement, scroller: HTMLElement, row: boolean): number {
  const box = el.getBoundingClientRect();
  const frame = scroller.getBoundingClientRect();
  const offset = box.top - frame.top + scroller.scrollTop;
  const pad = parseFloat(getComputedStyle(scroller).scrollPaddingTop) || 0;
  const want = row ? offset + box.height / 2 - scroller.clientHeight / 2 : offset - pad;
  return Math.max(0, Math.min(want, scroller.scrollHeight - scroller.clientHeight));
}

/** Flash a row with the selected row fill for 1.2 s. A second flash
 *  on the same row restarts it. */
function flash(el: HTMLElement) {
  window.clearTimeout(flashTimers.get(el));
  el.classList.remove('st-flash');
  // Read layout so the animation starts over.
  void el.offsetWidth;
  el.classList.add('st-flash');
  flashTimers.set(
    el,
    window.setTimeout(() => el.classList.remove('st-flash'), FLASH_MS),
  );
}

/** Scroll the first of `ids` that the page draws under `root` into
 *  view, and flash it when it is a row (it carries data-st-flash) or
 *  ring it when it is something to pick (data-st-coach).
 *  Returns a cleanup that stops looking and holding. */
export function revealSettingsAnchor(root: HTMLElement, ids: readonly string[]): () => void {
  let findTimer: number | undefined;
  let holdTimer: number | undefined;
  let stopped = false;
  // Your own scrolling or typing ends the hold.
  const release = () => {
    stopped = true;
  };
  const inputs = ['wheel', 'pointerdown', 'keydown', 'touchstart'] as const;
  for (const type of inputs) root.addEventListener(type, release, { passive: true });

  const find = (id: string) =>
    root.querySelector<HTMLElement>(`[data-st-anchor="${CSS.escape(id)}"]`);

  const reveal = (el: HTMLElement) => {
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const coach = el.getAttribute('data-st-coach');
    const row = el.hasAttribute('data-st-flash') || coach !== null;
    const scroller = scrollerFor(el, root);
    let placed = scrollTopFor(el, scroller, row);
    scroller.scrollTo({ top: placed, behavior: reduce ? 'auto' : 'smooth' });
    if (coach !== null) showCoach({ find: () => [el], line: coach });
    else if (row) flash(el);
    const started = Date.now();
    const hold = () => {
      if (stopped || !el.isConnected || Date.now() - started > HOLD_MS) return;
      const next = scrollTopFor(el, scroller, row);
      if (Math.abs(next - placed) > 1) {
        placed = next;
        scroller.scrollTo({ top: next, behavior: 'auto' });
      }
      holdTimer = window.setTimeout(hold, HOLD_EVERY_MS);
    };
    holdTimer = window.setTimeout(hold, HOLD_EVERY_MS);
  };

  let tries = 0;
  const step = () => {
    tries += 1;
    let el = ids.length > 0 ? find(ids[0]) : null;
    if (!el && tries >= SECTION_AFTER_TRIES) {
      for (const id of ids.slice(1)) {
        el = find(id);
        if (el) break;
      }
    }
    if (el) reveal(el);
    else if (tries < FIND_TRIES) findTimer = window.setTimeout(step, FIND_EVERY_MS);
  };
  step();

  return () => {
    stopped = true;
    window.clearTimeout(findTimer);
    window.clearTimeout(holdTimer);
    for (const type of inputs) root.removeEventListener(type, release);
  };
}
