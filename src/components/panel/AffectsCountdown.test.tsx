import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import panelCss from '../../styles/panel.css?raw';
import type { AffectInput, TrackedInput } from '../../lib/affectsView';
import type { PaneLeaf } from '../../lib/paneLayout';
import type { AffectsMarker } from '../../lib/session';
import { CountdownView } from './AffectsCountdown';
import { PaneLeafContext } from './paneActions';

// The stores behind AffectsPane reach the Tauri bridge. CountdownView,
// under test, draws from plain values and never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const LEAF: PaneLeaf = { id: 'affects-1', pane: 'affects', weight: 1, props: {} };
const aff = (name: string, duration: number | null): AffectInput => ({ name, duration });

const TRACKED: TrackedInput[] = [
  'mounted',
  'sanctuary',
  'bless',
  'armor',
  'shield',
  'stone skin',
  'fly',
  'levitate',
].map((name) => ({ name }));
const FOURTEEN = [
  aff('pass door', 8),
  aff('levitate', 44),
  aff('detect invis', 47),
  aff('sanctuary', 1),
  aff('haste', 14),
  aff('stone skin', 38),
  aff('shield', 31),
  aff('armor', 31),
  aff('fly', 2),
  aff('the Triumph of One God', 188),
  aff('mounted', -1),
  aff('virtues', -1),
  aff('totems canticle', 22),
  aff('bagatelle of bravado', 19),
];
const TWENTY = [
  ...FOURTEEN,
  aff('frenzy', 9),
  aff('protective shield', 6),
  aff('giant strength', 40),
  aff('detect magic', 45),
  aff('faerie fire', 3),
];
// Hours at full, the most seen since each was cast.
const FULL = { sanctuary: 10, fly: 53, armor: 48, shield: 48, 'pass door': 12, haste: 26 };
const BOARD_BOX = { width: 494, height: 191 };

function draw(
  current: AffectInput[] | null,
  options: {
    tracked?: TrackedInput[];
    hidden?: boolean;
    marker?: AffectsMarker;
    full?: Record<string, number>;
    box?: { width: number; height: number };
  } = {},
): string {
  return renderToStaticMarkup(
    <PaneLeafContext.Provider value={LEAF}>
      <CountdownView
        current={current}
        tracked={options.tracked ?? TRACKED}
        hidden={options.hidden ?? false}
        box={options.box ?? BOARD_BOX}
        marker={options.marker}
        full={options.full ?? FULL}
      />
    </PaneLeafContext.Provider>,
  );
}

/** Every cell as `mark name hours(tone) meter% words`, in order. */
function cellsOf(html: string): string[] {
  const out: string[] = [];
  const cell =
    /<li class="pane-countdown-cell pane-affect-(\w+)[^"]*"[^>]*>(?:<span class="pane-affect-mark is-(\w+)"[^>]*><\/span>)?<span class="pane-countdown-line"><span class="pane-countdown-name">([^<]*)(?:<span class="pane-sr">([^<]*)<\/span>)?<\/span><span class="pane-countdown-hours([^"]*)"[^>]*>([^<]*)<\/span><\/span>(?:<span class="pane-countdown-meter"[^>]*><span class="pane-countdown-fill" style="width:([^%]*)%"><\/span><\/span>)?<\/li>/g;
  for (const m of html.matchAll(cell)) {
    const tone = m[5].trim().replace('is-', '');
    const meter = m[7] === undefined ? 'no meter' : `${m[7]}%`;
    out.push(
      [m[2] ?? '.', m[3], `${m[6]}${tone ? `(${tone})` : ''}`, meter, m[4] ?? ''].join(' ').trim(),
    );
  }
  return out;
}

/** The declarations of one rule in panel.css. */
function rule(selector: string): string {
  const at = panelCss.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return panelCss.slice(at, panelCss.indexOf('}', at));
}

describe('CountdownView', () => {
  it('says the same sentences as Timers first in place of rows', () => {
    expect(draw(null)).toContain('<p class="pane-empty">Affects appear when you log in.</p>');
    const hidden = draw([], { hidden: true });
    expect(hidden).toContain('The game hides your affects right now.');
    expect(hidden).not.toContain('missing');
    expect(draw([], { tracked: [] })).toContain('Nothing affects you right now.');
  });

  it('draws board B: what you miss first, then everything by hours left', () => {
    const html = draw(FOURTEEN);
    expect(html).toContain('<span class="pane-meta pane-meta-danger">1 missing</span>');
    expect(html).toContain('<span class="pane-meta pane-meta-warn">2 running out</span>');
    expect(html).toContain('aria-label="Affects by hours left"');
    expect(cellsOf(html)).toEqual([
      'missing bless - 0.0% , missing',
      'danger sanctuary 1(danger) 10.0% , 1 hour, running out',
      'warn fly 2(warn) 3.8% , 2 hours, running out',
      '. pass door 8 66.7% , 8 hours',
      '. haste 14 53.8% , 14 hours',
      // No full yet reads as full.
      '. bagatelle of bravado 19 100.0% , 19 hours',
      '. totems canticle 22 100.0% , 22 hours',
      'up armor 31 64.6% , 31 hours',
      'up shield 31 64.6% , 31 hours',
      'up stone skin 38 100.0% , 38 hours',
      'up levitate 44 100.0% , 44 hours',
      '. detect invis 47 100.0% , 47 hours',
      '. the Triumph of One God 188 100.0% , 188 hours',
      'up mounted + 100.0% , permanent',
      '. virtues + 100.0% , permanent',
    ]);
    // Fifteen rows balance as eight over seven.
    expect(html).toContain('grid-template-rows:repeat(8, 23px)');
    expect(html).toContain('style="grid-row:1;grid-column:2"');
    expect(html).not.toContain('more</button>');
  });

  it('draws no meter for an affect the server sent no hours for', () => {
    const html = draw([aff('detect magic', null), aff('armor', 20)]);
    expect(cellsOf(html)).toContain('. detect magic  no meter');
  });

  it('counts the end of the countdown in the last cell of the page', () => {
    const html = draw(TWENTY);
    expect(html).toMatch(
      /<li class="pane-countdown-more-cell" style="grid-row:8;grid-column:2"><button type="button" class="pane-affects-more" aria-label="5 more affects, scroll to them">5 more<\/button><\/li>/,
    );
    expect(html).toContain('pane-countdown-window is-paged');
    // Every affect stays in the list for a screen reader.
    expect(cellsOf(html)).toHaveLength(20);
    expect(cellsOf(html)[3]).toBe('harmful faerie fire 3 100.0% , 3 hours, harmful');
  });

  it('draws one column in a narrow pane', () => {
    const html = draw(FOURTEEN, { box: { width: 247, height: 207 } });
    expect(html).toContain('grid-template-columns:repeat(1, minmax(0, 1fr))');
    // Split right, 247 by 235: eight rows and the count of seven more.
    expect(html).toContain('aria-label="7 more affects, scroll to them"');
  });

  it('names the marker on the body only when it is not the dot', () => {
    expect(draw(FOURTEEN)).toContain('<div class="pane-body">');
    expect(draw(FOURTEEN, { marker: 'none' })).toContain(
      '<div class="pane-body" data-affects-marker="none">',
    );
  });

  it('sets the geometry of the board', () => {
    expect(rule('.pane-countdown')).toContain('column-gap: 24px');
    expect(rule('.pane-countdown')).toContain('padding: 0 12px 0 18px');
    expect(rule('.pane-countdown-cell')).toContain('height: 23px');
    expect(rule('.pane-countdown-cell .pane-affect-mark')).toContain('top: 5px');
    expect(rule('.pane-countdown-line')).toContain('height: 17px');
    expect(rule('.pane-countdown-line')).toContain('padding: 1px 0 0 14px');
    expect(rule('.pane-countdown-meter')).toContain('top: 19px');
    expect(rule('.pane-countdown-meter')).toContain('background: var(--divider)');
    expect(rule('.pane-countdown-fill')).toContain('background: var(--tertiary)');
    expect(rule('.pane-countdown-cell.is-warn .pane-countdown-fill')).toContain('var(--warn)');
    expect(rule('.pane-countdown-cell.is-danger .pane-countdown-fill')).toContain('var(--danger)');
    expect(rule('.pane-countdown-hours.is-danger')).toContain('font-weight: 700');
  });
});
