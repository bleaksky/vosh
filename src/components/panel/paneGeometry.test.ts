import { describe, expect, it } from 'vitest';
import { defaultLayout, splitPane, type PaneSplit } from '../../lib/paneLayout';
import { distribute, dragSizes, layoutPanes, MIN_PANE_H } from './paneGeometry';

describe('distribute', () => {
  it('splits into whole pixels that sum to the total', () => {
    const sizes = distribute(663, [0.6, 0.4]);
    expect(sizes).toEqual([398, 265]);
    expect(sizes.reduce((a, b) => a + b, 0)).toBe(663);
  });

  it('keeps every share within a pixel across many siblings', () => {
    const sizes = distribute(100, [1, 1, 1]);
    expect(sizes.reduce((a, b) => a + b, 0)).toBe(100);
    for (const s of sizes) expect(Math.abs(s - 100 / 3)).toBeLessThan(1);
  });

  it('shares evenly when no weight is positive', () => {
    expect(distribute(90, [0, 0, 0])).toEqual([30, 30, 30]);
  });

  it('returns nothing for no children and zero for no space', () => {
    expect(distribute(100, [])).toEqual([]);
    expect(distribute(-5, [1, 1])).toEqual([0, 0]);
  });
});

describe('layoutPanes', () => {
  it('stacks the default map over affects with a 1 px handle between', () => {
    const { leaves, handles } = layoutPanes(defaultLayout().root, 300, 664);
    expect(leaves.map((l) => l.leaf.pane)).toEqual(['map', 'affects']);
    expect(leaves[0].rect).toEqual({ x: 0, y: 0, w: 300, h: 398 });
    expect(handles).toHaveLength(1);
    expect(handles[0]).toMatchObject({
      parentId: 'root',
      index: 0,
      dir: 'column',
      rect: { x: 0, y: 398, w: 300, h: 1 },
      sizes: [398, 265],
    });
    expect(leaves[1].rect).toEqual({ x: 0, y: 399, w: 300, h: 265 });
  });

  it('sets a split right pane beside its sibling inside the stack', () => {
    const tree = splitPane(defaultLayout().root, 'affects', 'row', 'group');
    const { leaves, handles } = layoutPanes(tree, 440, 664);
    const byPane = Object.fromEntries(leaves.map((l) => [l.leaf.pane, l.rect]));
    expect(byPane.map).toEqual({ x: 0, y: 0, w: 440, h: 398 });
    expect(byPane.affects).toEqual({ x: 0, y: 399, w: 220, h: 265 });
    expect(byPane.group).toEqual({ x: 221, y: 399, w: 219, h: 265 });
    const vertical = handles.find((h) => h.dir === 'row');
    expect(vertical?.rect).toEqual({ x: 220, y: 399, w: 1, h: 265 });
  });

  it('lays out nothing for an empty root', () => {
    const empty: PaneSplit = { id: 'root', split: 'column', weight: 1, children: [] };
    expect(layoutPanes(empty, 300, 600)).toEqual({ leaves: [], handles: [] });
  });
});

describe('dragSizes', () => {
  it('trades space between the two neighbours only', () => {
    expect(dragSizes([100, 200, 300], 1, 50, MIN_PANE_H)).toEqual([100, 250, 250]);
  });

  it('stops each neighbour at the minimum', () => {
    expect(dragSizes([100, 200], 0, -90, MIN_PANE_H)).toEqual([50, 250]);
    expect(dragSizes([100, 200], 0, 900, MIN_PANE_H)).toEqual([250, 50]);
  });

  it('leaves a pair with no room to give alone', () => {
    expect(dragSizes([40, 40], 0, 10, MIN_PANE_H)).toEqual([40, 40]);
  });
});
