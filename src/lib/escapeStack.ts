import { useEffect, useRef } from 'react';

// One Esc closes one surface, the one opened last. Each open surface
// pushes its close function here, and a single capture listener on the
// window runs the top entry, so a find bar that opened first can no
// longer swallow the Esc meant for a menu opened over it. With nothing
// on the stack Esc passes through untouched to the focused element,
// where the command line uses it to leave the scrollback split.
//
// A surface with Esc handling of its own (the palette steps back out of
// a submenu before it closes) names its element as the owner. An Esc
// pressed inside the top entry's owner goes on to that element's own
// handler instead of the close function.

type Owner = () => Element | null;
type Entry = { close: () => void; owner: Owner | undefined };

const stack: Entry[] = [];
let installed = false;

function onKeyDown(event: KeyboardEvent): void {
  if (event.key !== 'Escape' || event.isComposing) return;
  const top = stack[stack.length - 1];
  if (!top) return;
  const owner = top.owner?.();
  if (owner && event.target instanceof Node && owner.contains(event.target)) return;
  event.preventDefault();
  event.stopPropagation();
  top.close();
}

function install(): void {
  if (installed || typeof window === 'undefined') return;
  installed = true;
  window.addEventListener('keydown', onKeyDown, true);
}

/** Put `close` on top of the stack. Returns the function that takes it
 *  off again, which is safe to call more than once. */
export function pushEscape(close: () => void, owner?: Owner): () => void {
  install();
  const entry: Entry = { close, owner };
  stack.push(entry);
  return () => {
    const at = stack.lastIndexOf(entry);
    if (at >= 0) stack.splice(at, 1);
  };
}

/** Number of surfaces waiting on Esc. */
export function escapeDepth(): number {
  return stack.length;
}

/** Keep `close` on the stack while `open` is true. The latest `close`
 *  and `owner` always run, so callers can pass inline functions. */
export function useEscape(open: boolean, close: () => void, owner?: Owner): void {
  const closeRef = useRef(close);
  const ownerRef = useRef(owner);
  useEffect(() => {
    closeRef.current = close;
    ownerRef.current = owner;
  });
  useEffect(() => {
    if (!open) return;
    return pushEscape(
      () => closeRef.current(),
      () => ownerRef.current?.() ?? null,
    );
  }, [open]);
}
