import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

// The stack listens on the window. Node has no DOM, so a bare
// EventTarget stands in for it and a plain Event carries the key.
beforeAll(() => {
  vi.stubGlobal('window', new EventTarget());
});

function pressEscape(opts: { composing?: boolean } = {}): Event {
  const event = Object.assign(new Event('keydown', { cancelable: true }), {
    key: 'Escape',
    isComposing: opts.composing ?? false,
  });
  window.dispatchEvent(event);
  return event;
}

const cleanups: (() => void)[] = [];

afterEach(() => {
  while (cleanups.length > 0) cleanups.pop()?.();
});

describe('escape stack', () => {
  it('closes only the surface opened last', async () => {
    const { pushEscape, escapeDepth } = await import('./escapeStack');
    const closed: string[] = [];
    cleanups.push(pushEscape(() => closed.push('find')));
    const popMenu = pushEscape(() => {
      closed.push('menu');
      popMenu();
    });
    cleanups.push(popMenu);

    const first = pressEscape();
    expect(closed).toEqual(['menu']);
    expect(first.defaultPrevented).toBe(true);
    expect(escapeDepth()).toBe(1);

    pressEscape();
    expect(closed).toEqual(['menu', 'find']);
  });

  it('lets Esc through when nothing is open', async () => {
    const { escapeDepth } = await import('./escapeStack');
    expect(escapeDepth()).toBe(0);
    expect(pressEscape().defaultPrevented).toBe(false);
  });

  it('leaves Esc to an input method that is composing', async () => {
    const { pushEscape } = await import('./escapeStack');
    const close = vi.fn();
    cleanups.push(pushEscape(close));
    expect(pressEscape({ composing: true }).defaultPrevented).toBe(false);
    expect(close).not.toHaveBeenCalled();
  });

  it('leaves an Esc inside the owner to the owner', async () => {
    const { pushEscape } = await import('./escapeStack');
    const palette = new EventTarget() as EventTarget & { contains: (n: unknown) => boolean };
    const inside = { nodeType: 1 };
    palette.contains = (n) => n === inside;
    const close = vi.fn();
    cleanups.push(pushEscape(close, () => palette as unknown as Element));
    const event = Object.assign(new Event('keydown', { cancelable: true }), {
      key: 'Escape',
      isComposing: false,
    });
    Object.defineProperty(event, 'target', { value: inside });
    vi.stubGlobal('Node', Object);
    window.dispatchEvent(event);
    expect(close).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);

    pressEscape();
    expect(close).toHaveBeenCalledTimes(1);
  });

  it('takes an entry off once however often its remover runs', async () => {
    const { pushEscape, escapeDepth } = await import('./escapeStack');
    const remove = pushEscape(() => undefined);
    cleanups.push(pushEscape(() => undefined));
    remove();
    remove();
    expect(escapeDepth()).toBe(1);
  });
});
