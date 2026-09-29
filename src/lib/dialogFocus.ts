// Keeps the keyboard inside a modal dialog card. Tab and Shift+Tab
// cycle through the card's controls and never leave it, and focus that
// lands anywhere else in the window comes back to the first control.
// Enter on one of the card's controls presses that control. Enter on
// the card itself, or on its text, runs the dialog's confirm. Enter
// from anywhere outside the card stops there, so it neither confirms
// nor reaches the control it was pressed on.
//
// The listeners sit on the document in the capture phase. Node tests
// pass stand-ins for the document and the card.

/** The controls Tab stops on. */
export const DIALOG_FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** Something that takes focus. */
export interface Focusable {
  focus(options?: FocusOptions): void;
}

/** The card the trap holds focus in. An HTMLElement fits. */
export interface DialogCard extends Focusable {
  contains(node: Node | null): boolean;
  querySelectorAll(selectors: string): ArrayLike<Focusable>;
}

/** The index Tab moves to among `count` controls. `from` is -1 when
 *  focus is not on one of them. Wraps at both ends. */
export function nextTabStop(from: number, count: number, backward: boolean): number {
  if (count <= 0) return -1;
  if (from < 0) return backward ? count - 1 : 0;
  return (from + (backward ? -1 : 1) + count) % count;
}

/** Hold focus inside `card` and send Enter pressed inside it to
 *  `onConfirm`. Returns the function that lets go. */
export function trapDialogFocus(
  doc: EventTarget,
  card: DialogCard,
  onConfirm: () => void,
): () => void {
  const controls = (): Focusable[] => Array.from(card.querySelectorAll(DIALOG_FOCUSABLE));
  const inside = (target: EventTarget | null): boolean =>
    target !== null && card.contains(target as Node);
  const home = () => {
    const first = controls()[0];
    (first ?? card).focus({ preventScroll: true });
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'Tab') {
      event.preventDefault();
      event.stopPropagation();
      const list = controls();
      const at = list.indexOf(event.target as unknown as Focusable);
      const next = nextTabStop(at, list.length, event.shiftKey);
      (list[next] ?? card).focus({ preventScroll: true });
      return;
    }
    if (event.key !== 'Enter' || event.isComposing) return;
    // The focused control presses itself, so Enter on Cancel cancels.
    if (controls().includes(event.target as unknown as Focusable)) return;
    event.preventDefault();
    event.stopPropagation();
    if (inside(event.target)) onConfirm();
    else home();
  };

  const onFocusIn = (event: FocusEvent) => {
    if (!inside(event.target)) home();
  };

  const keyListener = onKeyDown as EventListener;
  const focusListener = onFocusIn as EventListener;
  // The options object, not a bare true. Node reads a bare true as no
  // capture when it removes a listener.
  const capture = { capture: true };
  doc.addEventListener('keydown', keyListener, capture);
  doc.addEventListener('focusin', focusListener, capture);
  return () => {
    doc.removeEventListener('keydown', keyListener, capture);
    doc.removeEventListener('focusin', focusListener, capture);
  };
}
