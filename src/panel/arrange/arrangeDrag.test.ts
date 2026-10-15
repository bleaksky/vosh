import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const { edgeIn, targetAt } = await import('./arrangeDrag');

describe('where a tile lands', () => {
  const panel = { x: 0, y: 0, w: 300, h: 200 };
  const sidebar = { x: 0, y: 0, w: 220, h: 200 };

  it('goes beside a pane wide enough for two from its outer quarters', () => {
    expect(edgeIn(panel, 20, 100)).toBe('left');
    expect(edgeIn(panel, 290, 100)).toBe('right');
    expect(edgeIn(panel, 150, 40)).toBe('before');
    expect(edgeIn(panel, 150, 160)).toBe('after');
  });

  it('goes above or below a pane too narrow for two', () => {
    expect(edgeIn(sidebar, 10, 40)).toBe('before');
    expect(edgeIn(sidebar, 210, 160)).toBe('after');
  });

  it('finds the shelf, a pane, an empty side, or nothing', () => {
    const targets = {
      leaves: [{ x: 900, y: 0, w: 300, h: 200, id: 'map', tree: 'panel' as const }],
      empty: [{ x: 0, y: 0, w: 220, h: 600, tree: 'sidebar' as const }],
      shelf: { x: 300, y: 100, w: 400, h: 120 },
    };
    expect(targetAt(targets, 400, 150)).toEqual({ kind: 'shelf' });
    expect(targetAt(targets, 1050, 30)).toEqual({
      kind: 'pane',
      drop: { tree: 'panel', at: 'map', edge: 'before' },
    });
    expect(targetAt(targets, 100, 300)).toEqual({
      kind: 'pane',
      drop: { tree: 'sidebar', at: null, edge: 'after' },
    });
    expect(targetAt(targets, 800, 500)).toBeNull();
  });
});
