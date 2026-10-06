import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { parseGroupInfo, type GroupInfo } from '../../stores/gmcp/groupStore';
import type { PaneLeaf } from '../paneLayout';
import { aabahranPacket } from '../../test/aabahranGmcp';
import { GroupPaneView } from './GroupPane';
import { PaneLeafContext } from '../paneActions';

// The stores behind GroupPane reach the Tauri bridge. GroupPaneView,
// under test, draws from plain values and never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const LEAF: PaneLeaf = { id: 'group-1', pane: 'group', weight: 1, props: {} };

function draw(group: GroupInfo): string {
  return renderToStaticMarkup(
    <PaneLeafContext.Provider value={LEAF}>
      <GroupPaneView group={group} />
    </PaneLeafContext.Provider>,
  );
}

const packet = (name: string) => parseGroupInfo(aabahranPacket(name).data);

describe('GroupPaneView', () => {
  it('draws one row per member with health', () => {
    const html = draw(packet('group-info.gmcp'));
    expect(html.match(/class="pane-row pane-member/g)).toHaveLength(2);
    expect(html).toContain('78%');
    expect(html).toContain('<span class="pane-meta pane-meta-quiet">2</span>');
  });

  it('waits for a group while you are solo', () => {
    expect(draw(packet('group-info-solo.gmcp'))).toContain('Group appears when you join a group.');
  });

  it('says the game hides your group and shows no health from before', () => {
    const html = draw(packet('group-info-hidden.gmcp'));
    expect(html).toContain('<p class="pane-empty">The game hides your group right now.</p>');
    expect(html).not.toContain('Group appears when you join a group.');
    expect(html).not.toContain('pane-member');
    expect(html).not.toContain('%');
    expect(html).not.toContain('pane-meta');
    // Even a roster that rode along never shows.
    const stale = draw({
      hidden: true,
      leader: 'Tester',
      members: [{ name: 'Tester', hp_pct: 5 }],
    });
    expect(stale).toContain('The game hides your group right now.');
    expect(stale).not.toContain('5%');
  });

  it('draws the roster again on the next Group.Info without the flag', () => {
    expect(draw(packet('group-info.gmcp'))).not.toContain('The game hides your group');
  });
});
