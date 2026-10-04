import { afterEach, describe, expect, it, vi } from 'vitest';

// Just what the store reads of a page: the face and the size on the
// root, the font loads, and the root's style changes.
function page(face: string, px = '') {
  const root = { face, px };
  const fonts = new EventTarget();
  let mutated: (() => void) | null = null;
  vi.stubGlobal('document', { documentElement: root, fonts });
  vi.stubGlobal('getComputedStyle', (el: typeof root) => ({
    getPropertyValue: (name: string) => {
      if (name === '--font-panel') return ` ${el.face}`;
      if (name === '--panel-text-px') return el.px;
      return '';
    },
  }));
  vi.stubGlobal(
    'MutationObserver',
    class {
      constructor(cb: () => void) {
        mutated = cb;
      }
      observe() {}
    },
  );
  return {
    setFace(next: string) {
      root.face = next;
      mutated?.();
    },
    setPx(next: string) {
      root.px = next;
      mutated?.();
    },
    loaded() {
      fonts.dispatchEvent(new Event('loadingdone'));
    },
  };
}

async function store() {
  vi.resetModules();
  return import('./panelFace');
}

describe('the panel face', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('reads the face the panes draw in off the root', async () => {
    page('"Iosevka", Menlo, monospace');
    const { readPanelFace } = await store();
    expect(readPanelFace()).toBe('"Iosevka", Menlo, monospace');
  });

  it('reads the panel size off the root, and 12 with none there', async () => {
    page('Menlo, monospace', ' 16');
    const { readPanelTextPx } = await store();
    expect(readPanelTextPx()).toBe(16);
    page('Menlo, monospace', '');
    expect((await store()).readPanelTextPx()).toBe(12);
    page('Menlo, monospace', 'big');
    expect((await store()).readPanelTextPx()).toBe(12);
  });

  it('falls back to a monospace face with nothing on the root', async () => {
    page('');
    const { readPanelFace } = await store();
    expect(readPanelFace()).toBe('ui-monospace, monospace');
  });

  it('counts up when a face loads and when the face changes, so measures run again', async () => {
    const at = page('Menlo, monospace');
    const { panelFaceVersion, subscribePanelFace } = await store();
    const heard = vi.fn();
    const stop = subscribePanelFace(heard);
    expect(panelFaceVersion()).toBe(0);

    at.loaded();
    expect(panelFaceVersion()).toBe(1);

    // A theme write changes the root's style but not the face.
    at.setFace('Menlo, monospace');
    expect(panelFaceVersion()).toBe(1);

    at.setFace('system-ui, sans-serif');
    expect(panelFaceVersion()).toBe(2);
    expect(heard).toHaveBeenCalledTimes(2);

    // A new panel size measures and draws again too.
    at.setPx('16');
    expect(panelFaceVersion()).toBe(3);
    at.setPx('16');
    expect(panelFaceVersion()).toBe(3);

    stop();
    at.loaded();
    expect(heard).toHaveBeenCalledTimes(3);
  });
});
