import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import panelCss from '../../styles/panel.css?raw';
import type { AffectInput, TrackedInput } from '../../lib/affectsView';
import type { PaneLeaf } from '../../lib/paneLayout';
import { ChipsView } from './AffectsChips';
import { FIXED_MEASURE } from './chipsGrid';
import { PaneLeafContext } from './paneActions';

// The stores behind AffectsPane reach the Tauri bridge. ChipsView,
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
const FULL = { sanctuary: 10, fly: 53, armor: 48, shield: 48, levitate: 44, haste: 26 };
const BOARD_BOX = { width: 494, height: 191 };

function draw(
  current: AffectInput[] | null,
  options: {
    tracked?: TrackedInput[];
    hidden?: boolean;
    full?: Record<string, number>;
    box?: { width: number; height: number };
  } = {},
): string {
  return renderToStaticMarkup(
    <PaneLeafContext.Provider value={LEAF}>
      <ChipsView
        current={current}
        tracked={options.tracked ?? TRACKED}
        hidden={options.hidden ?? false}
        box={options.box ?? BOARD_BOX}
        full={options.full ?? FULL}
        measure={FIXED_MEASURE}
      />
    </PaneLeafContext.Provider>,
  );
}

/** Every chip as `classes name hours(tone) --gauge words`, in order. */
function chipsOf(html: string): string[] {
  const out: string[] = [];
  const chip =
    /<li class="pane-chip ([^"]*)"(?: style="--gauge:([^"]*)")?><span class="pane-chip-name">([^<]*)(?:<span class="pane-sr">([^<]*)<\/span>)?<\/span>(?:<span class="pane-chip-hours([^"]*)"[^>]*>([^<]*)<\/span>)?<\/li>/g;
  for (const m of html.matchAll(chip)) {
    const tone = (m[5] ?? '').trim().replace('is-', '');
    out.push(
      [
        m[1],
        m[3],
        `${m[6] ?? ''}${tone ? `(${tone})` : ''}`,
        m[2] === undefined ? '' : `gauge ${m[2]}`,
        m[4] ?? '',
      ]
        .filter(Boolean)
        .join(' '),
    );
  }
  return out;
}

/** The group names each line shows, in order. */
const labelsOf = (html: string) =>
  [...html.matchAll(/<li class="pane-chips-label[^"]*" aria-hidden="true">([^<]*)<\/li>/g)].map(
    (m) => m[1],
  );

/** The declarations of one rule in panel.css. */
function rule(selector: string): string {
  const at = panelCss.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return panelCss.slice(at, panelCss.indexOf('}', at));
}

describe('ChipsView', () => {
  it('says the same sentences as the other styles in place of chips', () => {
    expect(draw(null)).toContain('<p class="pane-empty">Affects appear when you log in.</p>');
    const hidden = draw([], { hidden: true });
    expect(hidden).toContain('The game hides your affects right now.');
    expect(hidden).not.toContain('missing');
    expect(draw([], { tracked: [] })).toContain('Nothing affects you right now.');
  });

  it('draws board C: Recast, then Tracked, then Other, each chip in its state', () => {
    const html = draw(FOURTEEN);
    expect(html).toContain('<span class="pane-meta pane-meta-danger">1 missing</span>');
    expect(html).toContain('<span class="pane-meta pane-meta-warn">2 running out</span>');
    expect(labelsOf(html)).toEqual(['Recast', 'Tracked', '', 'Other', '', '']);
    expect(chipsOf(html)).toEqual([
      'pane-chip-missing bless - , missing',
      // Running out drains its gauge over the whole chip's tint.
      'pane-chip-tracked is-danger is-draining sanctuary 1(danger) gauge 0.1 , 1 hour, running out',
      'pane-chip-tracked is-warn is-draining fly 2(warn) gauge 0.0377 , 2 hours, running out',
      // Permanent is full and never drains.
      'pane-chip-tracked mounted + , permanent',
      'pane-chip-tracked is-draining armor 31 gauge 0.6458 , 31 hours',
      'pane-chip-tracked is-draining shield 31 gauge 0.6458 , 31 hours',
      // No full yet reads as a fresh cast.
      'pane-chip-tracked stone skin 38 , 38 hours',
      'pane-chip-tracked levitate 44 , 44 hours',
      // Other chips keep their ring and carry no gauge.
      'pane-chip-other pass door 8 , 8 hours',
      'pane-chip-other haste 14 , 14 hours',
      'pane-chip-other bagatelle of bravado 19 , 19 hours',
      'pane-chip-other totems canticle 22 , 22 hours',
      'pane-chip-other detect invis 47 , 47 hours',
      'pane-chip-other the Triumph of One God 188 , 188 hours',
      'pane-chip-other virtues + , permanent',
    ]);
    // Each line names its group for a screen reader.
    expect(html).toContain('<ul class="pane-chips-line" aria-label="Recast" style="top:4px">');
    expect(html).toContain('<ul class="pane-chips-line" aria-label="Tracked" style="top:32px">');
    expect(html).toContain('<ul class="pane-chips-line" aria-label="Other" style="top:84px">');
  });

  it('draws a running out chip at full exactly as the board, with no gauge over its tint', () => {
    const html = draw([aff('fly', 2), aff('sanctuary', 1)], { full: { fly: 2, sanctuary: 1 } });
    // Recast: what you miss in your order, then what runs out by hours.
    expect(chipsOf(html)).toEqual([
      'pane-chip-missing mounted - , missing',
      'pane-chip-missing bless - , missing',
      'pane-chip-missing armor - , missing',
      'pane-chip-missing shield - , missing',
      'pane-chip-missing stone skin - , missing',
      'pane-chip-missing levitate - , missing',
      'pane-chip-tracked is-danger sanctuary 1(danger) , 1 hour, running out',
      'pane-chip-tracked is-warn fly 2(warn) , 2 hours, running out',
    ]);
  });

  it('marks a harmful chip with its ring and an untracked one running out only in its hours', () => {
    const html = draw([aff('faerie fire', 3), aff('haste', 2)], { tracked: [] });
    expect(chipsOf(html)).toEqual([
      'pane-chip-harmful faerie fire 3 , 3 hours, harmful',
      'pane-chip-other haste 2(warn) , 2 hours',
    ]);
    // Nothing tracked: every chip is Other, and no group name shows.
    expect(labelsOf(html)).toEqual([]);
  });

  it('counts what does not fit after the last chip of the page', () => {
    const html = draw(TWENTY);
    expect(html).toMatch(
      /<li class="pane-chips-more"><button type="button" class="pane-affects-more" aria-label="2 more affects, scroll to them">2 more<\/button><\/li><\/ul><\/div><div class="pane-chips-page" style="height:191px">/,
    );
    expect(html).toContain('<div class="pane-chips is-paged" style="height:191px">');
    // Every affect stays in the list for a screen reader.
    expect(chipsOf(html)).toHaveLength(20);
    // The next page names the group it starts inside.
    expect(labelsOf(html).at(-1)).toBe('Other');
  });

  it('runs the group names in under 360 px', () => {
    const html = draw(FOURTEEN, { box: { width: 247, height: 207 } });
    expect(html).toContain(
      '<li class="pane-chips-label pane-chips-runin" aria-hidden="true">Recast</li>',
    );
  });

  it('draws the board and the gauge with theme tokens only', () => {
    expect(rule('.pane-chips-line')).toContain('left: 18px');
    expect(rule('.pane-chips-line')).toContain('right: 12px');
    expect(rule('.pane-chips-line')).toContain('gap: 4px');
    expect(rule('.pane-chips-label')).toContain('width: 56px');
    expect(rule('.pane-chip')).toContain('padding: 0 7px');
    // A chip at full is one flat ground, as the board draws it. Only a
    // draining chip lays the gauge over it.
    expect(rule('.pane-chip')).toContain('background: var(--chip-track)');
    expect(rule('.pane-chip.is-draining')).toContain('calc(var(--gauge, 1) * 100%)');
    expect(rule('.pane-chip-tracked')).toContain('--chip-track: color-mix(in srgb, var(--text) 8%');
    expect(rule('.pane-chip-tracked.is-draining')).toContain('--chip-edge: var(--sep)');
    // At full a running out chip is the board's tint alone.
    expect(rule('.pane-chip-tracked.is-warn')).toContain('var(--warn) 14%');
    expect(rule('.pane-chip-tracked.is-warn')).not.toContain('--chip-gauge');
    expect(rule('.pane-chip-tracked.is-warn.is-draining')).toContain('var(--warn) 20%');
    expect(rule('.pane-chip-tracked.is-danger')).toContain('var(--danger) 16%');
    expect(rule('.pane-chip-tracked.is-danger.is-draining')).toContain('var(--danger) 22%');
    expect(rule('.pane-chip-missing')).toContain('border: 1px dashed var(--danger)');
    expect(rule('.pane-chip-other')).toContain('--chip-edge: var(--sep)');
    expect(rule('.pane-chip-harmful')).toContain('var(--danger) 55%');
    const chips = panelCss.slice(
      panelCss.indexOf('/* Board Affects C'),
      panelCss.indexOf('/* ── Map'),
    );
    expect(chips).not.toMatch(/#[0-9a-f]{3,8}\b/i);
  });
});
