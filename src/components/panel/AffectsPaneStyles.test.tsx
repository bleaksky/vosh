import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PaneLeaf } from '../../lib/paneLayout';
import { DEFAULT_AFFECTS_DISPLAY, type AffectsDisplay } from '../../lib/session';
import type { CurrentAffect } from '../../lib/stores/affectsStore';
import { AffectsPane } from './AffectsPane';
import { PaneLeafContext } from './paneActions';

// AffectsPane reads the stores and draws the style you picked with the
// hours you set. The stores stand in here, so each style and threshold
// reaches the view it should without the running app.

const scene = vi.hoisted(() => ({
  display: null as AffectsDisplay | null,
  current: [] as CurrentAffect[],
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
vi.mock('../../lib/stores/affectsDisplayStore', () => ({
  useAffectsDisplay: () => scene.display,
}));
vi.mock('../../lib/stores/affectsStore', () => ({
  useAffects: () => scene.current,
  useAffectsHidden: () => false,
}));
vi.mock('../../lib/stores/trackedAffectsStore', () => ({
  useTrackedAffects: () => [
    { name: 'sanctuary', label: null },
    { name: 'bless', label: null },
    { name: 'fly', label: null },
  ],
}));
vi.mock('../../lib/stores/affectFullStore', () => ({
  useAffectFull: () => ({ fly: 10 }),
}));

const LEAF: PaneLeaf = { id: 'affects-1', pane: 'affects', weight: 1, props: {} };

const affect = (name: string, duration: number): CurrentAffect => ({
  name,
  kind: 'spell',
  duration,
  level: 50,
  modifiers: [],
});

function draw(display: Partial<AffectsDisplay>): string {
  scene.display = { ...DEFAULT_AFFECTS_DISPLAY, ...display };
  return renderToStaticMarkup(
    <PaneLeafContext.Provider value={LEAF}>
      <AffectsPane />
    </PaneLeafContext.Provider>,
  );
}

beforeEach(() => {
  scene.current = [affect('sanctuary', 4), affect('fly', 1), affect('haste', 9)];
});

describe('AffectsPane', () => {
  it('draws each style you pick', () => {
    expect(draw({ style: 'timers' })).toContain('class="pane-affects-grid"');
    expect(draw({ style: 'countdown' })).toContain('class="pane-countdown"');
    const chips = draw({ style: 'chips' });
    expect(chips).toContain('class="pane-chips');
    expect(chips).not.toContain('data-chip-fill');
    // Draining chips is the chips pane with the drain fill on its body.
    const drain = draw({ style: 'chips_drain' });
    expect(drain).toContain('<div class="pane-body" data-chip-fill="drain">');
    // Every chip is the one Grouped chips draws. Only the CSS differs.
    const strip = (html: string) => html.replace(' data-chip-fill="drain"', '');
    expect(strip(drain)).toBe(chips);
  });

  it('colors every style by the hours you set', () => {
    for (const style of ['timers', 'countdown', 'chips', 'chips_drain'] as const) {
      const before = draw({ style });
      expect(before, style).toContain('1 running out');
      // Sanctuary at 4 hours runs out once you set 4.
      const after = draw({ style, running_out: 4, almost_gone: 1 });
      expect(after, style).toContain('2 running out');
      expect(after, style).toContain(', 4 hours, running out');
      // Fly at 1 hour is red either way.
      expect(after, style).toContain('is-danger');
      // At 0 and 0 only an affect at none turns red, and none is.
      const none = draw({ style, running_out: 0, almost_gone: 0 });
      expect(none, style).not.toContain('running out</span>');
      expect(none, style).not.toContain('is-danger');
    }
  });
});
