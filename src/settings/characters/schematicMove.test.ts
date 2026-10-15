import { describe, expect, it } from 'vitest';
import { allPanes, sanitizeLayout, type PaneLayout } from '../../panel/paneLayout';
import { paneLabel } from '../../panel/paneTypes';
import {
  SCHEMATIC_HEIGHT,
  SCHEMATIC_WIDTH,
  VITALS_LINE,
  paneSchematic,
  sidebarSchematic,
} from './paneSchematic';
import { boxSide, dropAt, keyMove, movable, type SchematicBox } from './schematicMove';

// Moving panes on the drawing under Panel layout: where a drag lands
// and what marks it, and what each arrow key does.

const leaf = (pane: string) => ({ id: pane, pane });
const lay = (panel: string[], side: string[]): PaneLayout =>
  sanitizeLayout({
    root: { split: 'column', children: panel.map(leaf) },
    sidebar: { split: 'column', children: side.map(leaf) },
  });

/** The two boxes as the page lays them for a panel on the right: the
 *  sidebar at 0, the panel at 124. */
function boxes(layout: PaneLayout): SchematicBox[] {
  return [
    {
      tree: 'sidebar',
      x: 0,
      bottom: SCHEMATIC_HEIGHT,
      drawing: sidebarSchematic(layout.sidebar, paneLabel, 'Maren'),
    },
    {
      tree: 'panel',
      x: 124,
      bottom: VITALS_LINE,
      drawing: paneSchematic(layout.root, paneLabel, 'Maren'),
    },
  ];
}

describe('dropAt', () => {
  const layout = lay(['map', 'affects'], ['stats']);
  const b = boxes(layout);

  it('lands above a pane in its top half and below it in its bottom half', () => {
    // Map and Affects share the panel evenly, so Map fills rows 0 to 44.
    expect(dropAt(b, 200, 10, 'stats')).toEqual({
      drop: { tree: 'panel', at: 'map', edge: 'before' },
      wash: { x: 124, y: 0, width: SCHEMATIC_WIDTH, height: 22.5 },
      line: { x: 124, y: 0, width: SCHEMATIC_WIDTH, height: 2 },
    });
    expect(dropAt(b, 200, 40, 'stats')?.drop).toEqual({
      tree: 'panel',
      at: 'map',
      edge: 'after',
    });
    // The line below straddles the line between Map and Affects.
    expect(dropAt(b, 200, 40, 'stats')?.line.y).toBe(44);
  });

  it('lands in the sidebar beside its pane, the line inside the box at its foot', () => {
    const below = dropAt(b, 60, 100, 'map');
    expect(below?.drop).toEqual({ tree: 'sidebar', at: 'stats', edge: 'after' });
    expect(below?.line.y).toBe(SCHEMATIC_HEIGHT - 2);
  });

  it('lands nowhere over the pane on its way, over Vitals, or between the boxes', () => {
    expect(dropAt(b, 200, 10, 'map')).toBeNull();
    expect(dropAt(b, 200, 100, 'stats')).toBeNull();
    expect(dropAt(b, 118, 40, 'stats')).toBeNull();
  });

  it('washes a box with no panes whole', () => {
    const empty = boxes(lay(['map', 'affects'], []));
    expect(dropAt(empty, 60, 80, 'map')).toEqual({
      drop: { tree: 'sidebar', at: null, edge: 'after' },
      wash: { x: 1, y: 1, width: 110, height: SCHEMATIC_HEIGHT - 1 },
      line: { x: 1, y: 1, width: 110, height: 2 },
    });
  });
});

describe('keyMove', () => {
  const trees = (l: PaneLayout | null) => (l ? [allPanes(l.root), allPanes(l.sidebar)] : null);

  it('moves a pane past the one before or after it with Up and Down', () => {
    const layout = lay(['map', 'affects', 'group'], []);
    expect(trees(keyMove(layout, 'group', 'ArrowUp', 'right'))).toEqual([
      ['map', 'group', 'affects'],
      [],
    ]);
    expect(trees(keyMove(layout, 'map', 'ArrowDown', 'right'))).toEqual([
      ['affects', 'map', 'group'],
      [],
    ]);
    expect(keyMove(layout, 'map', 'ArrowUp', 'right')).toBeNull();
    expect(keyMove(layout, 'group', 'ArrowDown', 'right')).toBeNull();
  });

  it('moves a pane to the other box with the arrow that points at it', () => {
    const layout = lay(['map', 'affects'], ['stats']);
    // The panel on the right, so the sidebar is to the left.
    expect(trees(keyMove(layout, 'map', 'ArrowLeft', 'right'))).toEqual([
      ['affects'],
      ['stats', 'map'],
    ]);
    expect(keyMove(layout, 'map', 'ArrowRight', 'right')).toBeNull();
    expect(trees(keyMove(layout, 'stats', 'ArrowRight', 'right'))).toEqual([
      ['map', 'affects', 'stats'],
      [],
    ]);
    // Swapped, the arrows swap too.
    expect(trees(keyMove(layout, 'map', 'ArrowRight', 'left'))).toEqual([
      ['affects'],
      ['stats', 'map'],
    ]);
  });

  it('leaves the writing card where it is, and ignores other keys', () => {
    const layout = lay(['map', 'writing'], []);
    expect(movable(layout, 'writing')).toBe(false);
    expect(movable(layout, 'map')).toBe(true);
    expect(keyMove(layout, 'writing', 'ArrowUp', 'right')).toBeNull();
    expect(keyMove(layout, 'map', 'Enter', 'right')).toBeNull();
  });
});

describe('boxSide', () => {
  it('puts the sidebar across from the panel', () => {
    expect(boxSide('panel', 'right')).toBe('right');
    expect(boxSide('sidebar', 'right')).toBe('left');
    expect(boxSide('sidebar', 'left')).toBe('right');
  });
});
