import { afterEach, describe, expect, it, vi } from 'vitest';

// Just what the store reads of a page: the face on the root, the font
// loads, and the root's style changes.
function page(face: string) {
  const root = { face };
  const fonts = new EventTarget();
  let mutated: (() => void) | null = null;
  vi.stubGlobal('document', { documentElement: root, fonts });
  vi.stubGlobal('getComputedStyle', (el: typeof root) => ({
    getPropertyValue: (name: string) => (name === '--font-panel' ? ` ${el.face}` : ''),
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

    stop();
    at.loaded();
    expect(heard).toHaveBeenCalledTimes(2);
  });
});
