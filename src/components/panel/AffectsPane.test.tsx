import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { PaneLeaf } from '../../lib/paneLayout';
import { groupCurrentAffects, type CurrentAffect } from '../../lib/stores/affectsStore';
import type { TrackedAffect } from '../../lib/session';
import { aabahranPacket } from '../../test/aabahranGmcp';
import { AffectsPaneView } from './AffectsPane';
import { PaneLeafContext } from './paneActions';

// The stores behind AffectsPane reach the Tauri bridge. AffectsPaneView,
// under test, draws from plain values and never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const LEAF: PaneLeaf = { id: 'affects-1', pane: 'affects', weight: 1, props: {} };
const TRACKED: TrackedAffect[] = [
  { name: 'sanctuary', label: null },
  { name: 'bless', label: null },
];

function draw(current: CurrentAffect[] | null, hidden: boolean): string {
  return renderToStaticMarkup(
    <PaneLeafContext.Provider value={LEAF}>
      <AffectsPaneView current={current} tracked={TRACKED} hidden={hidden} />
    </PaneLeafContext.Provider>,
  );
}

const list = (name: string) => groupCurrentAffects(aabahranPacket(name).data);

describe('AffectsPaneView', () => {
  it('waits for your affects before the first list', () => {
    expect(draw(null, false)).toContain('Affects appear when you log in.');
  });

  it('says the game hides your affects and marks nothing missing', () => {
    const html = draw(list('char-affects-hidden.gmcp'), true);
    expect(html).toContain('<p class="pane-empty">The game hides your affects right now.</p>');
    expect(html).not.toContain('missing');
    expect(html).not.toContain('pane-row');
    expect(html).not.toContain('Nothing affects you right now.');
  });

  it('draws the checklist again on the next list without the flag', () => {
    const html = draw(list('char-affects.gmcp'), false);
    expect(html).not.toContain('The game hides your affects');
    expect(html).toContain('1 missing');
    expect(html).toContain('pane-affect-missing');
    expect(html).toContain('bless');
    expect(html).not.toContain('Bless');
  });

  it('tells an empty list the game shows from one it hides', () => {
    expect(draw([], false)).toContain('pane-affect-missing');
    expect(draw([], false)).toContain('2 missing');
  });
});
