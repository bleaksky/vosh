// Keyboard focus that stays in a container, such as the prompt card,
// while the control that had it goes away. The card swaps its parts as
// you work: the part's rows remount as you pick another, Remove takes its
// button with it, the picker closes, a step gives way to the next. Focus
// then falls to the page, and every key the card reads stops reaching it.
// The keeper hands focus back to the container then, and only then: focus
// you send elsewhere, to the terminal, a menu or nowhere, stays there.

export interface FocusKeeper {
  /** Take focus back if the control that had it is gone. Run it after
   *  each render that may have removed one. */
  check: () => void;
  stop: () => void;
}

export function keepFocus(
  container: HTMLElement,
  doc: Document = document,
  later: (fn: () => void) => void = (fn) => void setTimeout(fn, 0),
): FocusKeeper {
  // Focus is inside, or was until the control that had it went away.
  let inside = false;
  const lost = () => doc.activeElement === null || doc.activeElement === doc.body;
  const check = () => {
    if (inside && lost()) container.focus({ preventScroll: true });
  };
  const onIn = () => {
    inside = true;
  };
  const onOut = (e: FocusEvent) => {
    const to = e.relatedTarget as Node | null;
    if (to && container.contains(to)) return;
    const from = e.target as Node | null;
    later(() => {
      // A control still on the page lost focus because you moved it.
      if (from?.isConnected) inside = false;
      else check();
    });
  };
  container.addEventListener('focusin', onIn);
  container.addEventListener('focusout', onOut);
  return {
    check,
    stop: () => {
      container.removeEventListener('focusin', onIn);
      container.removeEventListener('focusout', onOut);
    },
  };
}
