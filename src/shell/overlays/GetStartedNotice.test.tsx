import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

// First Run board 1: Get started folded to its notice in the corner, on
// the update notice recipe, and its slot in the stack of Q19.

vi.mock('@tauri-apps/api/core', () => ({ invoke: async () => null }));

let CornerNotices: typeof import('./CornerNotices').CornerNotices;
let store: typeof import('../getStarted/getStartedStore');
let fix: typeof import('../../stores/presetFixStore');
let toasts: typeof import('../../stores/toasts');

beforeAll(async () => {
  // A toast keeps its dismiss timer on window, and the saved world sits
  // in storage.
  vi.stubGlobal('window', globalThis);
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => undefined });
  ({ CornerNotices } = await import('./CornerNotices'));
  store = await import('../getStarted/getStartedStore');
  fix = await import('../../stores/presetFixStore');
  toasts = await import('../../stores/toasts');
});

afterEach(() => {
  fix.closePresetFix();
  for (const t of toasts.getToasts()) toasts.dismissToast(t.id);
});

/** Open Get started, finish `done` and fold it to the notice. */
function folded(done: ('connect' | 'presets')[]) {
  store.openList();
  for (const id of done) store.markDone(id);
  store.fold();
}

describe('the Get started notice', () => {
  it('counts what you finished, with Close and Open', () => {
    folded(['connect']);
    const html = renderToStaticMarkup(<CornerNotices />);
    expect(html).toContain('<span class="ov-update-msg">Get started</span>');
    expect(html).toContain('<span class="ov-update-meta">1 of 5 done</span>');
    expect(html).toMatch(/>Close<\/button><button[^>]*class="ov-button is-primary"[^>]*>Open</);
  });

  it('shows nothing while the card is open or shut', () => {
    folded([]);
    store.unfold();
    expect(renderToStaticMarkup(<CornerNotices />)).toBe('<div class="ov-corner"></div>');
    store.end();
    expect(renderToStaticMarkup(<CornerNotices />)).not.toContain('Get started</span>');
  });

  it('sits under a preset fix and over the update notice', async () => {
    folded(['connect']);
    fix.showPresetFix({
      told: [{ preset: 'disarm_buff_fade', trigger: 'disarm.secondary', row: 'send' }],
      removed: [],
    });
    const html = renderToStaticMarkup(<CornerNotices />);
    expect(html.indexOf('Get started</span>')).toBeGreaterThan(html.indexOf('ov-update is-warn'));
    const source = (await import('./CornerNotices?raw')).default;
    const order = [
      'Notice: PresetFixNotice',
      'Notice: GetStartedNotice',
      'Notice: UpdateNotice',
    ].map((s) => source.indexOf(s));
    expect(order).toEqual([...order].sort((a, b) => a - b));
    expect(order[0]).toBeGreaterThan(0);
  });

  it('ends Get started at Close with the toast from board 1', () => {
    folded([]);
    store.end();
    expect(store.getGetStarted().shows).toBe('shut');
    expect(toasts.getToasts().at(-1)).toMatchObject({
      message: 'Get started closed',
      meta: 'Help opens it again',
    });
  });
});
