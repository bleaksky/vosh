import { describe, expect, it, vi } from 'vitest';
import { dropBoxes, edgeAt } from './paneDrag';

vi.mock('./paneActions', () => ({ dropPane: vi.fn() }));

// Where a dragged pane lands against the pane under the pointer, and
// what marks it: the half on that side washed, a 2 px line on its edge.

const box = { x: 10, y: 100, w: 280, h: 200 };

describe('edgeAt', () => {
  it('joins above in the top half and below in the bottom half', () => {
    expect(edgeAt(100, box)).toBe('before');
    expect(edgeAt(199, box)).toBe('before');
    expect(edgeAt(200, box)).toBe('after');
    expect(edgeAt(299, box)).toBe('after');
  });
});

describe('dropBoxes', () => {
  it('washes the top half with a line on the top edge for a drop above', () => {
    expect(dropBoxes(box, 'before')).toEqual({
      wash: { x: 10, y: 100, w: 280, h: 100 },
      line: { x: 10, y: 99, w: 280, h: 2 },
    });
  });

  it('washes the bottom half with a line on the bottom edge for a drop below', () => {
    expect(dropBoxes(box, 'after')).toEqual({
      wash: { x: 10, y: 200, w: 280, h: 100 },
      line: { x: 10, y: 298, w: 280, h: 2 },
    });
  });
});
