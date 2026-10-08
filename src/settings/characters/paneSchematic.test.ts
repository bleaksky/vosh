import { describe, expect, it } from 'vitest';
import { defaultLayout, sanitize, type PaneSplit, type PaneType } from '../../panel/paneLayout';
import { paneLabel as labelFor } from '../../panel/paneTypes';
import { paneSchematic, schematicSentence } from './paneSchematic';

// A tree in the wire shape, repaired the way the backend's is.
function tree(raw: unknown): PaneSplit {
  return sanitize(raw);
}

const leaf = (pane: PaneType, weight = 1) => ({ id: pane, pane, weight });

describe('paneSchematic', () => {
  it('draws the stock map over affects tree with Vitals along the bottom', () => {
    const s = paneSchematic(defaultLayout().root, labelFor, 'Ilsabet');
    expect(s.width).toBe(180);
    expect(s.height).toBe(110);
    // Map takes 0.525 of the 89 rows above the Vitals line.
    expect(s.lines).toBe('M1 47.5H179M1 89.5H179');
    expect(s.regions.map((r) => [r.pane, r.rect])).toEqual([
      ['map', { x: 0, y: 0, width: 180, height: 47 }],
      ['affects', { x: 0, y: 48, width: 180, height: 41 }],
    ]);
    expect(s.labels).toEqual([
      { x: 10, y: 16, text: 'Map' },
      { x: 10, y: 64, text: 'Affects' },
      { x: 10, y: 103.5, text: 'Vitals' },
    ]);
    expect(s.ariaLabel).toBe(
      "Ilsabet's panel. Map on top, Affects below it, Vitals along the bottom.",
    );
  });

  it('matches the board when the split sits where the board draws it', () => {
    // The line sits at 43.5, a map share of 43 in 89.
    const s = paneSchematic(
      tree({ split: 'column', children: [leaf('map', 43), leaf('affects', 46)] }),
      labelFor,
      'Ilsabet',
    );
    expect(s.lines).toBe('M1 43.5H179M1 89.5H179');
    expect(s.labels.map((l) => [l.text, l.x, l.y])).toEqual([
      ['Map', 10, 16],
      ['Affects', 10, 60],
      ['Vitals', 10, 103.5],
    ]);
  });

  it('draws side by side panes with a vertical line down to Vitals', () => {
    const s = paneSchematic(
      tree({ split: 'row', children: [leaf('group'), leaf('chat')] }),
      labelFor,
      'Healer',
    );
    expect(s.lines).toBe('M90.5 1V89M1 89.5H179');
    expect(s.regions.map((r) => r.rect)).toEqual([
      { x: 0, y: 0, width: 90, height: 89 },
      { x: 91, y: 0, width: 89, height: 89 },
    ]);
    expect(s.labels.slice(0, 2)).toEqual([
      { x: 10, y: 16, text: 'Group' },
      { x: 101, y: 16, text: 'Chat' },
    ]);
    expect(s.ariaLabel).toBe(
      "Healer's panel. Group on the left, Chat on the right, Vitals along the bottom.",
    );
  });

  it('draws nested splits inside their parent region', () => {
    // Map on top, then group and chat side by side under it.
    const s = paneSchematic(
      tree({
        split: 'column',
        children: [
          leaf('map', 0.5),
          { id: 'bottom', split: 'row', weight: 0.5, children: [leaf('group'), leaf('chat')] },
        ],
      }),
      labelFor,
      'Ilsabet',
    );
    // Half of 89 rounds up to 45.
    expect(s.lines).toBe('M1 45.5H179M90.5 46V89M1 89.5H179');
    expect(s.regions.map((r) => [r.pane, r.rect])).toEqual([
      ['map', { x: 0, y: 0, width: 180, height: 45 }],
      ['group', { x: 0, y: 46, width: 90, height: 43 }],
      ['chat', { x: 91, y: 46, width: 89, height: 43 }],
    ]);
    expect(s.ariaLabel).toBe(
      "Ilsabet's panel. Map on top, Group and Chat side by side below it, Vitals along the bottom.",
    );
  });

  it('keeps every pane at least a pixel when a share rounds to nothing', () => {
    const s = paneSchematic(
      tree({
        split: 'column',
        children: [leaf('map', 0.0001), leaf('affects', 0.0001), leaf('group', 0.9998)],
      }),
      labelFor,
      'Ilsabet',
    );
    const heights = s.regions.map((r) => r.rect.height);
    expect(heights.every((h) => h >= 1)).toBe(true);
    // Too short to label, so only the group and Vitals labels show.
    expect(s.labels.map((l) => l.text)).toEqual(['Group', 'Vitals']);
  });

  it('centers the label in a short row instead of leaving it out', () => {
    const s = paneSchematic(
      tree({
        split: 'column',
        children: [leaf('affects', 2), leaf('group', 2), leaf('chat', 0.8)],
      }),
      labelFor,
      'Ilsabet',
    );
    const chat = s.regions.find((r) => r.pane === 'chat')?.rect;
    expect(chat?.height).toBeLessThan(22);
    expect(chat?.height).toBeGreaterThanOrEqual(12);
    const label = s.labels.find((l) => l.text === 'Chat');
    expect(label).toEqual({
      x: 10,
      y: (chat?.y ?? 0) + (chat?.height ?? 0) / 2 + 3.5,
      text: 'Chat',
    });
    // A tall row keeps its label 16 down from its top.
    expect(s.labels[0]).toEqual({ x: 10, y: 16, text: 'Affects' });
  });

  it('leaves out a label the region is too narrow for', () => {
    const s = paneSchematic(
      tree({
        split: 'row',
        children: [leaf('map'), leaf('imm'), leaf('affects')],
      }),
      labelFor,
      'Ilsabet',
    );
    expect(s.labels.map((l) => l.text)).toEqual(['Map', 'Affects', 'Vitals']);
    expect(s.ariaLabel).toBe(
      "Ilsabet's panel. Map on the left, Staff queues in the middle, Affects on the right, Vitals along the bottom.",
    );
  });

  it('describes a panel with no panes and one with a single pane', () => {
    const empty = paneSchematic(
      { id: 'root', split: 'column', weight: 1, children: [] },
      labelFor,
      'Default',
    );
    expect(empty.regions).toEqual([]);
    expect(empty.lines).toBe('M1 89.5H179');
    expect(empty.labels).toEqual([{ x: 10, y: 103.5, text: 'Vitals' }]);
    expect(empty.ariaLabel).toBe("Default's panel. No panes, only Vitals along the bottom.");

    const single = tree({ split: 'column', children: [leaf('chat')] });
    expect(schematicSentence(single, labelFor, 'Default')).toBe(
      "Default's panel. Chat fills the panel, Vitals along the bottom.",
    );
  });

  it('names four panes across and splits down inside a split across', () => {
    const four = tree({
      split: 'row',
      children: [leaf('map'), leaf('group'), leaf('chat'), leaf('affects')],
    });
    expect(schematicSentence(four, labelFor, 'Ilsabet')).toBe(
      "Ilsabet's panel. Map on the left, Group next to it, Chat next to it, Affects on the right, Vitals along the bottom.",
    );
    const mixed = tree({
      split: 'row',
      children: [
        { id: 'left', split: 'column', weight: 1, children: [leaf('map'), leaf('affects')] },
        leaf('chat'),
      ],
    });
    expect(schematicSentence(mixed, labelFor, 'Ilsabet')).toBe(
      "Ilsabet's panel. Map over Affects on the left, Chat on the right, Vitals along the bottom.",
    );
  });
});
