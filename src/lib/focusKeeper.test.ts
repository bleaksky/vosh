import { describe, expect, it, vi } from 'vitest';
import { keepFocus } from './focusKeeper';

// A container that keeps keyboard focus when the control that held it
// goes away, as the prompt card's controls do as you work.

type Listener = (e: { target: unknown; relatedTarget: unknown }) => void;

function fakes() {
  const listeners = new Map<string, Listener>();
  const doc = { body: { name: 'body' }, activeElement: null as unknown };
  const inside = new Set<unknown>();
  const container = {
    focus: vi.fn(() => {
      doc.activeElement = container;
    }),
    contains: (node: unknown) => node === container || inside.has(node),
    addEventListener: (type: string, fn: Listener) => listeners.set(type, fn),
    removeEventListener: (type: string) => listeners.delete(type),
  };
  const control = (connected = true) => {
    const c = { isConnected: connected };
    inside.add(c);
    return c;
  };
  const queue: (() => void)[] = [];
  const keeper = keepFocus(container as unknown as HTMLElement, doc as unknown as Document, (fn) =>
    queue.push(fn),
  );
  const fire = (type: string, target: unknown, relatedTarget: unknown = null) =>
    listeners.get(type)?.({ target, relatedTarget });
  const flush = () => queue.splice(0).forEach((fn) => fn());
  return { doc, container, control, keeper, fire, flush, listeners };
}

describe('keepFocus', () => {
  it('takes focus back when the control that had it goes away', () => {
    const f = fakes();
    const bold = f.control();
    f.fire('focusin', bold);
    f.doc.activeElement = bold;
    // The control unmounts and focus falls to the body.
    bold.isConnected = false;
    f.doc.activeElement = f.doc.body;
    f.keeper.check();
    expect(f.container.focus).toHaveBeenCalledWith({ preventScroll: true });
  });

  it('takes it back when the page says the gone control lost focus', () => {
    const f = fakes();
    const remove = f.control();
    f.fire('focusin', remove);
    remove.isConnected = false;
    f.doc.activeElement = f.doc.body;
    f.fire('focusout', remove);
    f.flush();
    expect(f.container.focus).toHaveBeenCalledTimes(1);
  });

  it('lets focus go where you sent it', () => {
    const f = fakes();
    const bold = f.control();
    f.fire('focusin', bold);
    // You clicked the terminal, or a menu outside the card took focus.
    f.fire('focusout', bold, { name: 'terminal' });
    f.doc.activeElement = { name: 'terminal' };
    f.flush();
    f.keeper.check();
    // A click on nothing leaves focus on the body, still not here.
    const italic = f.control();
    f.fire('focusin', italic);
    f.fire('focusout', italic);
    f.doc.activeElement = f.doc.body;
    f.flush();
    f.keeper.check();
    expect(f.container.focus).not.toHaveBeenCalled();
  });

  it('stops listening', () => {
    const f = fakes();
    f.keeper.stop();
    expect(f.listeners.size).toBe(0);
  });
});
