import { describe, expect, it, vi } from 'vitest';
import { allPanes, defaultLayout, type PaneLayout } from '../panel/paneLayout';

vi.mock('../panel/panelLayoutStore', () => ({ updatePanelLayout: vi.fn() }));

const { withWritingPane } = await import('./pinnedPane');

describe('withWritingPane', () => {
  it('adds the pane once at the foot of the panel and shows the panel', () => {
    const hidden: PaneLayout = { ...defaultLayout(), panel_open: false };
    const pinned = withWritingPane(hidden, true);
    expect(pinned.panel_open).toBe(true);
    expect(allPanes(pinned.root)).toEqual(['map', 'affects', 'writing']);
    expect(withWritingPane(pinned, true)).toBe(pinned);
  });

  it('gives the pane a share for six rows and the card around them', () => {
    const pinned = withWritingPane(defaultLayout(), true);
    const writing = pinned.root.children.at(-1);
    expect(writing?.weight).toBeGreaterThan(0.3);
    expect(writing?.weight).toBeLessThan(0.5);
  });

  it('takes the pane out and leaves the rest and the panel as they were', () => {
    const pinned = withWritingPane(defaultLayout(), true);
    const floated = withWritingPane(pinned, false);
    expect(allPanes(floated.root)).toEqual(['map', 'affects']);
    expect(floated.panel_open).toBe(true);
    expect(withWritingPane(floated, false)).toBe(floated);
  });
});
