import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { dismissToast, getToasts, pushToast } from './toasts';

describe('toasts', () => {
  const setTimeout = vi.fn(() => 1);
  beforeEach(() => {
    setTimeout.mockClear();
    vi.stubGlobal('window', { setTimeout, clearTimeout: vi.fn() });
  });
  afterEach(() => {
    for (const toast of getToasts()) dismissToast(toast.id);
    vi.unstubAllGlobals();
  });

  it('dismisses a toast on a timer', () => {
    pushToast({ kind: 'info', message: 'Saved' });
    expect(setTimeout).toHaveBeenCalledTimes(1);
  });

  it('keeps a meta set in the terminal face, such as prompt codes (P14)', () => {
    pushToast({
      kind: 'info',
      message: 'Vosh reads your new prompt.',
      meta: '%h ',
      metaMono: true,
    });
    pushToast({ kind: 'success', message: 'Connected', meta: 'example.net:4000' });
    expect(getToasts().map((t) => t.metaMono)).toEqual([true, undefined]);
  });

  it('keeps a sticky toast up until you close it', () => {
    const id = pushToast({ kind: 'info', message: 'Quit Vosh now', sticky: true });
    expect(setTimeout).not.toHaveBeenCalled();
    expect(getToasts().map((t) => t.id)).toEqual([id]);
    dismissToast(id);
    expect(getToasts()).toEqual([]);
  });

  it('keeps the one button a toast carries', () => {
    const run = vi.fn();
    pushToast({
      kind: 'info',
      message: 'Your other Chat pane now shows Everything else.',
      action: { label: 'Undo', run },
    });
    pushToast({ kind: 'info', message: 'Saved' });
    expect(getToasts().map((t) => t.action?.label)).toEqual(['Undo', undefined]);
  });
});
