import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { dismissToast, getToasts, pushToast } from '../../stores/toasts';
import { Toasts } from './Toasts';

vi.mock('../../ipc/nativeSurface', () => ({ onNativeCopied: () => Promise.resolve(() => {}) }));

describe('Toasts', () => {
  beforeEach(() => {
    vi.stubGlobal('window', { setTimeout: vi.fn(() => 1), clearTimeout: vi.fn() });
  });
  afterEach(() => {
    for (const toast of getToasts()) dismissToast(toast.id);
    vi.unstubAllGlobals();
  });

  it('draws a toast with a button as a card and a plain one as a button', () => {
    pushToast({ kind: 'info', message: 'Saved' });
    pushToast({ kind: 'info', message: 'Moved', action: { label: 'Undo', run: () => {} } });
    const html = renderToStaticMarkup(<Toasts />);
    expect(html).toContain('<button type="button" class="ov-toast is-info" title="Dismiss">');
    expect(html).toMatch(
      /<div class="ov-toast has-action is-info">.*Moved.*<button type="button" class="ov-button">Undo<\/button><\/div>/,
    );
  });
});
