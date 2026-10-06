import { afterEach, describe, expect, it, vi } from 'vitest';
import { nextTabStop, trapDialogFocus, type DialogCard, type Focusable } from './dialogFocus';

// Node has no DOM. A bare EventTarget stands in for the document, plain
// objects for the card and its buttons, and `focused` records where
// focus went.

let focused: unknown = null;

function control(name: string): Focusable & { name: string } {
  const node = {
    name,
    focus: () => {
      focused = node;
    },
  };
  return node;
}

function setup() {
  const doc = new EventTarget();
  const cancel = control('cancel');
  const confirm = control('confirm');
  const behind = control('behind');
  const inside: unknown[] = [cancel, confirm];
  const card: DialogCard & { name: string } = {
    name: 'card',
    focus: () => {
      focused = card;
    },
    contains: (node) => (node as unknown) === card || inside.includes(node),
    querySelectorAll: () => [cancel, confirm],
  };
  const onConfirm = vi.fn();
  const release = trapDialogFocus(doc, card, onConfirm);
  cleanups.push(release);
  return { doc, card, cancel, confirm, behind, onConfirm, release };
}

function press(
  doc: EventTarget,
  target: unknown,
  key: string,
  opts: { shift?: boolean } = {},
): Event {
  const event = new Event('keydown', { cancelable: true });
  Object.defineProperties(event, {
    key: { value: key },
    shiftKey: { value: opts.shift ?? false },
    isComposing: { value: false },
    target: { value: target },
  });
  doc.dispatchEvent(event);
  return event;
}

function focusIn(doc: EventTarget, target: unknown): void {
  const event = new Event('focusin');
  Object.defineProperty(event, 'target', { value: target });
  doc.dispatchEvent(event);
}

const cleanups: (() => void)[] = [];

afterEach(() => {
  while (cleanups.length > 0) cleanups.pop()?.();
  focused = null;
});

describe('nextTabStop', () => {
  it('steps forward and back and wraps at both ends', () => {
    expect(nextTabStop(0, 2, false)).toBe(1);
    expect(nextTabStop(1, 2, false)).toBe(0);
    expect(nextTabStop(0, 2, true)).toBe(1);
    expect(nextTabStop(1, 2, true)).toBe(0);
  });

  it('enters at the first control, or the last going back', () => {
    expect(nextTabStop(-1, 3, false)).toBe(0);
    expect(nextTabStop(-1, 3, true)).toBe(2);
    expect(nextTabStop(-1, 0, false)).toBe(-1);
  });
});

describe('trapDialogFocus', () => {
  it('cycles Tab through the card and never leaves it', () => {
    const { doc, cancel, confirm } = setup();
    const tab = press(doc, cancel, 'Tab');
    expect(tab.defaultPrevented).toBe(true);
    expect(focused).toBe(confirm);
    press(doc, confirm, 'Tab');
    expect(focused).toBe(cancel);
    press(doc, cancel, 'Tab', { shift: true });
    expect(focused).toBe(confirm);
  });

  it('brings Tab from behind the card back into it', () => {
    const { doc, cancel, confirm, behind } = setup();
    press(doc, behind, 'Tab');
    expect(focused).toBe(cancel);
    press(doc, behind, 'Tab', { shift: true });
    expect(focused).toBe(confirm);
  });

  it('brings focus that lands behind the card back to the first control', () => {
    const { doc, cancel, confirm, behind } = setup();
    focusIn(doc, behind);
    expect(focused).toBe(cancel);
    focused = confirm;
    focusIn(doc, confirm);
    expect(focused).toBe(confirm);
  });

  it('never confirms on Enter pressed on a control behind the card', () => {
    const { doc, cancel, behind, onConfirm } = setup();
    const enter = press(doc, behind, 'Enter');
    expect(onConfirm).not.toHaveBeenCalled();
    expect(enter.defaultPrevented).toBe(true);
    expect(focused).toBe(cancel);
  });

  it('leaves Enter on a card button to that button', () => {
    const { doc, cancel, confirm, onConfirm } = setup();
    expect(press(doc, cancel, 'Enter').defaultPrevented).toBe(false);
    expect(press(doc, confirm, 'Enter').defaultPrevented).toBe(false);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it('confirms on Enter pressed on the card itself', () => {
    const { doc, card, onConfirm } = setup();
    const enter = press(doc, card, 'Enter');
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(enter.defaultPrevented).toBe(true);
  });

  it('lets go of every key when released', () => {
    const { doc, behind, onConfirm, release } = setup();
    release();
    expect(press(doc, behind, 'Enter').defaultPrevented).toBe(false);
    expect(press(doc, behind, 'Tab').defaultPrevented).toBe(false);
    focusIn(doc, behind);
    expect(focused).toBe(null);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
