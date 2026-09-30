import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import panelCss from '../../styles/panel.css?raw';
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

function draw(
  current: CurrentAffect[] | null,
  hidden: boolean,
  tracked: TrackedAffect[] = TRACKED,
  box?: { width: number; height: number },
): string {
  return renderToStaticMarkup(
    <PaneLeafContext.Provider value={LEAF}>
      <AffectsPaneView current={current} tracked={tracked} hidden={hidden} box={box} />
    </PaneLeafContext.Provider>,
  );
}

const affect = (name: string, duration: number): CurrentAffect => ({
  name,
  kind: 'spell',
  duration,
  level: 50,
  modifiers: [],
});

// Erelei on the approved board: his eight tracked affects in his order,
// and what the game sends while bless has worn off.
const ERELEI_TRACKED: TrackedAffect[] = [
  'mounted',
  'sanctuary',
  'bless',
  'armor',
  'shield',
  'stone skin',
  'fly',
  'levitate',
].map((name) => ({ name, label: null }));
const ERELEI: CurrentAffect[] = [
  affect('pass door', 8),
  affect('levitate', 44),
  affect('detect invis', 47),
  affect('sanctuary', 1),
  affect('haste', 14),
  affect('stone skin', 38),
  affect('shield', 31),
  affect('armor', 31),
  affect('fly', 2),
  affect('the Triumph of One God', 188),
  affect('mounted', -1),
  affect('virtues', -1),
  affect('totems canticle', 22),
  affect('bagatelle of bravado', 19),
];
// The pane on the board, 494 by 219: a 191 px body under the header.
const BOARD_BOX = { width: 494, height: 191 };

/** Every cell as `mark hours name words`, in document order. */
function cellsOf(html: string): string[] {
  const out: string[] = [];
  const cell =
    /<li class="pane-affect pane-affect-(\w+)[^"]*"[^>]*>(?:<span class="pane-affect-mark is-(\w+)"[^>]*><\/span>)?<span class="pane-affect-hours([^"]*)"[^>]*>([^<]*)<\/span><span class="pane-affect-name">([^<]*)<span class="pane-sr">([^<]*)<\/span>/g;
  for (const m of html.matchAll(cell)) {
    const tone = m[3].trim().replace('is-', '');
    out.push([m[2] ?? '.', `${m[4]}${tone ? `(${tone})` : ''}`, m[5], m[6]].join(' ').trim());
  }
  return out;
}

/** The declarations of one rule in panel.css. */
function rule(selector: string): string {
  const at = panelCss.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return panelCss.slice(at, panelCss.indexOf('}', at));
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
    expect(html).not.toContain('<li');
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

  it('draws the approved board, your slots in your order and the rest under a hairline', () => {
    const html = draw(ERELEI, false, ERELEI_TRACKED, BOARD_BOX);
    expect(html).toContain('<span class="pane-meta pane-meta-danger">1 missing</span>');
    expect(html).toContain('<span class="pane-meta pane-meta-warn">2 running out</span>');
    expect(cellsOf(html)).toEqual([
      'up + mounted , permanent',
      'danger 1(danger) sanctuary , 1 hour, running out',
      'missing - bless , missing',
      'up 31 armor , 31 hours',
      'up 31 shield , 31 hours',
      'up 38 stone skin , 38 hours',
      'warn 2(warn) fly , 2 hours, running out',
      'up 44 levitate , 44 hours',
      '. 8 pass door , 8 hours',
      '. 14 haste , 14 hours',
      '. 19 bagatelle of bravado , 19 hours',
      '. 22 totems canticle , 22 hours',
      '. 47 detect invis , 47 hours',
      '. 188 the Triumph of One God , 188 hours',
      '. + virtues , permanent',
    ]);
    expect(html.match(/class="pane-affects-rule"/g)).toHaveLength(1);
    // The rest fill down the left column, then down the right.
    expect(html).toContain('style="grid-row:1;grid-column:1"');
    expect(html).toContain('style="grid-row:1;grid-column:2"');
    // A hairline, not a Not tracked label, sets the rest apart.
    expect(html).not.toContain('<h3');
    expect(html).not.toContain('more</button>');
  });

  it('counts the affects that do not fit in the last cell', () => {
    const fight = [
      ...ERELEI,
      affect('faerie fire', 3),
      affect('protective shield', 6),
      affect('frenzy', 9),
      affect('giant strength', 40),
      affect('detect magic', 45),
    ];
    const html = draw(fight, false, ERELEI_TRACKED, BOARD_BOX);
    const cells = cellsOf(html);
    expect(cells[8]).toBe('harmful 3 faerie fire , 3 hours, harmful');
    expect(html).toContain('aria-label="5 more affects, scroll to them"');
    expect(html).toContain('>5 more</button>');
    // Every affect stays in the list for a screen reader.
    expect(cells).toHaveLength(20);
  });

  it('shows your label in place of the name, exactly as you wrote it', () => {
    const html = draw([affect('sanctuary', 9)], false, [{ name: 'sanctuary', label: 'sanc' }]);
    expect(cellsOf(html)).toEqual(['up 9 sanc , 9 hours']);
  });

  it('sets the names and the hours in the terminal face', () => {
    expect(rule('.pane-affect-name')).toContain('font-family: var(--font-mud');
    expect(rule('.pane-affect-hours')).toContain('font-family: var(--font-mud');
    expect(rule('.pane-affect-hours')).toContain('text-align: right');
    expect(rule('.pane-affect-hours.is-danger')).toContain('font-weight: 700');
  });
});
