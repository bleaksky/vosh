import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import affectsCss from '../../styles/affects.css?raw';
import type { AffectInput, TrackedInput } from './affectsView';
import { deriveChrome } from '../../theme/chrome';
import { composite, contrast, parseHex } from '../../theme/color';
import { BUILTIN_THEMES, themeTokens } from '../../theme/themes';
import type { PaneLeaf } from '../paneLayout';
import { ChipsView, type ChipFill } from './AffectsChips';
import { chipDots, chipDotsPath, chipWidth, FIXED_MEASURE } from './chipsGrid';
import { PaneLeafContext } from '../paneActions';
import { PaneTextSizeContext } from '../paneTextSize';

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
    thresholds?: { runningOut: number; almostGone: number };
    fill?: ChipFill;
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
        thresholds={options.thresholds}
        fill={options.fill}
      />
    </PaneLeafContext.Provider>,
  );
}

/** Every chip as `classes name hours(tone) --gauge words`, in order. */
function chipsOf(html: string): string[] {
  const out: string[] = [];
  const chip =
    /<li class="pane-chip ([^"]*)"(?: style="--gauge:([^"]*)")?><span class="pane-chip-name">([^<]*)(?:<span class="visually-hidden">([^<]*)<\/span>)?<\/span>(?:<span class="pane-chip-hours([^"]*)"[^>]*>([^<]*)<\/span>)?(?:<svg class="pane-chip-dots"[^>]*>.*?<\/svg>)?<\/li>/g;
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

/** The declarations of one rule in affects.css. */
function rule(selector: string): string {
  const at = affectsCss.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return affectsCss.slice(at, affectsCss.indexOf('}', at));
}

/** The same, with the selector on one line however the sheet wraps
 *  it. */
function flatRule(selector: string): string {
  const flat = affectsCss.replace(/\s+/g, ' ');
  const at = flat.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return flat.slice(at, flat.indexOf('}', at));
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

  it('groups and tints what to recast by the hours you set', () => {
    const edges = [
      aff('sanctuary', 5),
      aff('armor', 6),
      aff('shield', 2),
      aff('fly', 0),
      aff('mounted', -1),
      aff('haste', 4),
    ];
    const tracked = ['sanctuary', 'bless', 'armor', 'shield', 'fly', 'mounted'].map((name) => ({
      name,
    }));
    // Sanctuary has no full yet, the most seen is unknown, so it reads full.
    const full = { armor: 48, shield: 8, fly: 53 };
    const html = draw(edges, { tracked, full, thresholds: { runningOut: 5, almostGone: 2 } });
    expect(html).toContain('<span class="pane-meta pane-meta-warn">3 running out</span>');
    expect(chipsOf(html)).toEqual([
      'pane-chip-missing bless - , missing',
      // An affect at none is empty, and its gauge with it.
      'pane-chip-tracked is-danger is-draining fly 0(danger) gauge 0 , 0 hours, running out',
      // Exactly at almost gone.
      'pane-chip-tracked is-danger is-draining shield 2(danger) gauge 0.25 , 2 hours, running out',
      // Exactly at running out, at full.
      'pane-chip-tracked is-warn sanctuary 5(warn) , 5 hours, running out',
      'pane-chip-tracked is-draining armor 6 gauge 0.125 , 6 hours',
      'pane-chip-tracked mounted + , permanent',
      'pane-chip-other haste 4(warn) , 4 hours',
    ]);
    expect(labelsOf(html)).toEqual(['Recast', 'Tracked', 'Other']);
    // At two and one, sanctuary goes back to Tracked and shield is yellow.
    expect(chipsOf(draw(edges, { tracked, full })).slice(0, 4)).toEqual([
      'pane-chip-missing bless - , missing',
      'pane-chip-tracked is-danger is-draining fly 0(danger) gauge 0 , 0 hours, running out',
      'pane-chip-tracked is-warn is-draining shield 2(warn) gauge 0.25 , 2 hours, running out',
      'pane-chip-tracked sanctuary 5 , 5 hours',
    ]);
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

  it('sets a run in name on a line of its own, hidden from a screen reader', () => {
    const html = draw(FOURTEEN, {
      tracked: [{ name: 'the Triumph of One God' }, ...TRACKED],
      box: { width: 247, height: 400 },
    });
    expect(html).toContain(
      '<div class="pane-chips-line" aria-hidden="true" style="top:56px"><span class="pane-chips-label pane-chips-runin">Tracked</span></div>' +
        '<ul class="pane-chips-line" aria-label="Tracked" style="top:80px"><li class="pane-chip pane-chip-tracked">',
    );
  });

  it('draws the board and the gauge with theme tokens only', () => {
    expect(rule('.pane-chips-line')).toContain('left: 18px');
    expect(rule('.pane-chips-line')).toContain('right: 12px');
    expect(rule('.pane-chips-line')).toContain('gap: 4px');
    // The gutter is 56 px at a 12 px panel and grows with the size.
    expect(rule('.pane-chips-label')).toContain('width: var(--mud-chip-gutter)');
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
    // Missing: soft round dots, never the dashed edge it had.
    expect(rule('.pane-chip-missing')).not.toContain('border');
    expect(rule('.pane-chip-missing')).toContain('position: relative');
    expect(rule('.pane-chip-dots')).toContain('inset: 0');
    expect(rule('.pane-chip-dots')).toContain('pointer-events: none');
    expect(rule('.pane-chip-dots rect')).toContain('stroke: var(--danger)');
    expect(rule('.pane-chip-dots rect')).toContain('stroke-width: 1.5px');
    expect(rule('.pane-chip-dots rect')).toContain('stroke-linecap: round');
    expect(rule('.pane-chip-dots rect')).toContain('fill: none');
    expect(rule('.pane-chip-other')).toContain('--chip-edge: var(--sep)');
    expect(rule('.pane-chip-harmful')).toContain('var(--danger) 55%');
    const chips = affectsCss.slice(affectsCss.indexOf('/* Board Affects C'));
    expect(chips).not.toMatch(/#[0-9a-f]{3,8}\b/i);
    expect(chips).not.toContain('dashed');
  });

  it('rings a missing chip in soft round dots that close evenly', () => {
    const html = draw([aff('sanctuary', 9)], {
      tracked: [{ name: 'bless' }, { name: 'sanctuary' }],
    });
    const ring = /<li class="pane-chip pane-chip-missing">.*?(<svg[^>]*>(.*?)<\/svg>)<\/li>/.exec(
      html,
    );
    expect(ring).not.toBeNull();
    expect(ring![1]).toContain('class="pane-chip-dots" aria-hidden="true"');
    // In a test the chip is never measured, so the ring takes the width
    // the chip was packed at.
    const width = chipWidth('bless', '-', FIXED_MEASURE);
    expect(width).toBe(64);
    const { count, gap } = chipDots(width);
    const { w, length } = chipDotsPath(width);
    expect(w).toBe(62.5);
    // 2w + 2h - 8r + 2 pi r, with h 18.5 and r 3.25.
    expect(length).toBeCloseTo(2 * 62.5 + 2 * 18.5 - 8 * 3.25 + 2 * Math.PI * 3.25, 6);
    expect(count).toBe(52);
    expect(gap * count).toBeCloseTo(length, 6);
    expect(ring![2]).toBe(
      `<rect x="0.75" y="0.75" width="62.5" height="18.5" rx="3.25" stroke-dasharray="0 ${
        Math.round(gap * 1000) / 1000
      }" stroke-dashoffset="${Math.round((gap / 2) * 1000) / 1000}"></rect>`,
    );
    // The chip keeps its name, its mark and its words for a screen reader.
    expect(chipsOf(html)[0]).toBe('pane-chip-missing bless - , missing');
  });

  it('keeps the dots on a pitch close to 3 px at every chip width', () => {
    for (const width of [24, 40, 64, 97, 160, 333]) {
      const { count, gap } = chipDots(width);
      expect(count, `${width}`).toBe(Math.round(chipDotsPath(width).length / 3));
      expect(Math.abs(gap - 3), `${width}`).toBeLessThan(0.1);
    }
  });
});

describe('Draining chips', () => {
  it('draws the same chips, groups and pages as Grouped chips, with the drain fill on the body', () => {
    for (const list of [FOURTEEN, TWENTY]) {
      const tint = draw(list);
      const drain = draw(list, { fill: 'drain' });
      expect(drain).toContain('<div class="pane-body" data-chip-fill="drain">');
      expect(tint).toContain('<div class="pane-body">');
      expect(drain.replace(' data-chip-fill="drain"', '')).toBe(tint);
    }
  });

  it('colors only the share of a chip running out that matches its hours, over a hairline', () => {
    const at = (selector: string) => rule(`[data-chip-fill='drain'] ${selector}`);
    // The share: the gauge as wide as the hours left, the whole chip at
    // full, over no ground.
    const ground = at(
      ".pane-chip-tracked.is-warn,\n[data-chip-fill='drain'] .pane-chip-tracked.is-danger",
    );
    expect(ground).toContain('--chip-track: transparent');
    expect(ground).toContain('--chip-edge: var(--sep)');
    expect(ground).toContain('calc(var(--gauge, 1) * 100%)');
    // Yellow and red at 30 percent, at full and while draining alike.
    const warn = at(
      ".pane-chip-tracked.is-warn,\n[data-chip-fill='drain'] .pane-chip-tracked.is-warn.is-draining",
    );
    expect(warn).toContain('--chip-gauge: color-mix(in srgb, var(--warn) 30%, transparent)');
    expect(warn).toContain('--chip-track: transparent');
    expect(warn).toContain('--chip-edge: var(--sep)');
    const danger = at(
      ".pane-chip-tracked.is-danger,\n[data-chip-fill='drain'] .pane-chip-tracked.is-danger.is-draining",
    );
    expect(danger).toContain('--chip-gauge: color-mix(in srgb, var(--danger) 30%, transparent)');
    expect(danger).toContain('--chip-track: transparent');
    expect(danger).toContain('--chip-edge: var(--sep)');
    // The drain rules come after Grouped chips', so they win at equal
    // weight, and touch only the tracked chips running out.
    const drainAt = affectsCss.indexOf("[data-chip-fill='drain']");
    expect(drainAt).toBeGreaterThan(
      affectsCss.indexOf('.pane-chip-tracked.is-danger.is-draining {'),
    );
    // Each selector on one line, however the sheet wraps it. A light
    // theme changes only the red fill and the yellow hours, by the
    // theme's own appearance, never by its id.
    const drainCss = affectsCss
      .slice(drainAt)
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .replace(/\s+/g, ' ');
    const selectors = [...drainCss.matchAll(/([^{}]+)\{[^}]*\}/g)].flatMap((m) =>
      m[1].split(',').map((s) => s.trim()),
    );
    expect(selectors).toHaveLength(8);
    for (const s of selectors) {
      expect(s).toMatch(
        /^(:root\[data-appearance='light'\] )?\[data-chip-fill='drain'\] \.pane-chip-tracked\.is-(warn|danger)(\.is-draining)?( \.pane-chip-hours)?$/,
      );
    }
  });

  // The hours a chip running out shows, on every built in theme. They sit
  // at the chip's right end. In Grouped chips that end is the plain track,
  // 14 percent yellow or 16 percent red, at full and while the gauge over
  // the left share stops short of it, and the hours are the warn tone in
  // yellow and the danger text tone in bold red. In Draining chips they
  // sit on the fill while the chip is at full, has no gauge or has no full
  // yet, and on the bare panel once the fill draws back past them.
  //
  // A dark theme draws the hours as Grouped chips does over a 30 percent
  // fill, and they read harder there than on the track. A light theme's
  // fill darkens toward the hours, so there the yellow hours take the
  // deeper warn text tone and the red fill eases to 24 percent. Every
  // built in theme and tone then reads at 3 to 1 or better over the fill
  // and over the bare panel. An imported light palette whose red sits at
  // or near 4.5 to 1 draws its hours close to the fill color over itself.
  // The common ones hold 3 to 1 at 24 percent, where 28 drops Catppuccin
  // Latte under it, as the next test pins. On a light theme Grouped chips draws its
  // yellow hours in warn text too, so no theme reads under 3 to 1 there.
  const GROUPED_UNDER_THREE: string[] = [];
  // How far the fill stands off the bare panel on a light theme, so the
  // drain stays clear. The Grouped chips track stands off it 1.15 to 1.25.
  const LIGHT_FILL_FLOOR = 1.3;
  // The light fills under that floor. Melange Light's red is a dusty rose
  // the window lifts only to the 3 to 1 status floor, so its red drain
  // stands a little nearer the paper, still further off than the 16
  // percent Grouped chips track.
  const LIGHT_FILL_UNDER: Record<string, string> = { 'melange-light danger': '1.27' };
  // The hours' contrast on each theme, on the Grouped chips track and
  // then on the Draining chips fill, over the panel the one ground rule
  // puts on the terminal ground.
  const HOURS_READ: Record<string, string> = {
    'obsidian-ember warn': '10.23 to 6.51',
    'obsidian-ember danger': '6.94 to 5.10',
    'triad warn': '11.53 to 6.71',
    'triad danger': '7.04 to 4.99',
    'rubric warn': '6.13 to 4.64',
    'rubric danger': '3.86 to 3.50',
    'kanso-zen warn': '8.90 to 5.74',
    'kanso-zen danger': '5.04 to 3.94',
    'tokyo-night warn': '6.48 to 4.47',
    'tokyo-night danger': '5.01 to 3.84',
    'nord warn': '5.77 to 3.93',
    'nord danger': '4.11 to 3.49',
    'rose-pine warn': '7.92 to 5.14',
    'rose-pine danger': '4.80 to 3.72',
    'gruvbox warn': '6.34 to 4.26',
    'gruvbox danger': '4.81 to 3.95',
    'catppuccin warn': '8.90 to 5.40',
    'catppuccin danger': '5.32 to 3.92',
    'dracula warn': '10.70 to 6.17',
    'dracula danger': '4.91 to 3.75',
    'monokai warn': '6.43 to 4.31',
    'monokai danger': '4.91 to 4.17',
    'one-half-dark warn': '5.91 to 4.05',
    'one-half-dark danger': '4.62 to 3.73',
    'solarized-dark warn': '5.16 to 3.81',
    'solarized-dark danger': '5.15 to 4.47',
    'solarized-light warn': '3.95 to 3.34',
    'solarized-light danger': '3.63 to 3.21',
    'tango-dark warn': '6.98 to 4.49',
    'tango-dark danger': '4.89 to 4.06',
    'classic-vivid warn': '13.29 to 7.38',
    'classic-vivid danger': '4.60 to 3.81',
    'high-contrast warn': '10.74 to 6.50',
    'high-contrast danger': '7.51 to 5.32',
    'high-contrast-light warn': '7.15 to 5.38',
    'high-contrast-light danger': '5.85 to 4.96',
    'everforest-dark warn': '5.08 to 3.57',
    'everforest-dark danger': '4.53 to 3.63',
    'green-screen warn': '12.78 to 7.21',
    'green-screen danger': '5.21 to 4.08',
    'srcery warn': '9.52 to 5.93',
    'srcery danger': '4.67 to 3.71',
    'nightfly warn': '8.51 to 5.48',
    'nightfly danger': '5.06 to 4.02',
    'melange-dark warn': '6.44 to 4.30',
    'melange-dark danger': '4.44 to 3.51',
    'melange-light warn': '3.79 to 3.07',
    'melange-light danger': '3.90 to 3.59',
    'modus-vivendi warn': '10.67 to 6.79',
    'modus-vivendi danger': '6.34 to 4.87',
    'harbor-dark warn': '7.58 to 5.09',
    'harbor-dark danger': '7.27 to 5.12',
    'iceberg-dark warn': '7.11 to 4.76',
    'iceberg-dark danger': '5.44 to 4.07',
  };

  it('reads the hours at 3 to 1 or better over the fill and the bare panel, pinned per theme', () => {
    // The track under the hours in Grouped chips, as the sheet draws it.
    expect(rule('.pane-chip-tracked.is-warn')).toContain(
      '--chip-track: color-mix(in srgb, var(--warn) 14%, transparent)',
    );
    expect(rule('.pane-chip-tracked.is-danger')).toContain(
      '--chip-track: color-mix(in srgb, var(--danger) 16%, transparent)',
    );
    expect(rule('.pane-chip-hours.is-warn')).toContain('color: var(--warn)');
    expect(rule('.pane-chip-hours.is-danger')).toContain('color: var(--danger-text)');
    // A light theme's red fill and yellow hours in Draining chips. The
    // yellow fill keeps the 30 percent every theme draws.
    const light = (selector: string) =>
      flatRule(`:root[data-appearance='light'] [data-chip-fill='drain'] ${selector}`);
    expect(
      light(
        ".pane-chip-tracked.is-danger, :root[data-appearance='light'] [data-chip-fill='drain'] .pane-chip-tracked.is-danger.is-draining",
      ),
    ).toContain('--chip-gauge: color-mix(in srgb, var(--danger) 24%, transparent)');
    expect(
      flatRule(":root[data-appearance='light'] .pane-chip-tracked.is-warn .pane-chip-hours"),
    ).toContain('color: var(--warn-text)');
    const read: Record<string, string> = {};
    const groupedUnder: string[] = [];
    const fillUnder: Record<string, string> = {};
    for (const theme of BUILTIN_THEMES) {
      const t = themeTokens(theme);
      const isLight = t.appearance === 'light';
      const hex = (value: string) => {
        const rgb = parseHex(value);
        expect(rgb, `${theme.id} ${value}`).not.toBeNull();
        return rgb!;
      };
      const panel = hex(t.panel);
      const text = hex(t.text);
      for (const [tone, color, groupedHours, drainHours, track, fill] of [
        [
          'warn',
          hex(t.warn),
          hex(isLight ? t.warnText : t.warn),
          hex(isLight ? t.warnText : t.warn),
          0.14,
          0.3,
        ],
        ['danger', hex(t.danger), hex(t.dangerText), hex(t.dangerText), 0.16, isLight ? 0.24 : 0.3],
      ] as const) {
        const key = `${theme.id} ${tone}`;
        const grouped = contrast(groupedHours, composite(color, panel, track));
        const drained = composite(color, panel, fill);
        const draining = contrast(drainHours, drained);
        // Dark themes stay as drawn, and the fill reads harder than the
        // track.
        if (!isLight) expect(draining, key).toBeLessThan(grouped);
        // Over the fill and over the bare panel alike, 3 to 1 or better.
        expect(draining, key).toBeGreaterThanOrEqual(3);
        expect(contrast(drainHours, panel), `${key} bare`).toBeGreaterThanOrEqual(3);
        // Once the fill draws back past them, the bare panel reads
        // better than the Grouped chips track.
        expect(contrast(drainHours, panel), key).toBeGreaterThan(grouped);
        // The name stays clear of the fill.
        expect(contrast(text, drained), `${key} name`).toBeGreaterThanOrEqual(3);
        // On a light theme the drain stays clear of the bare panel.
        if (isLight) {
          const off = contrast(drained, panel);
          if (off < LIGHT_FILL_FLOOR) fillUnder[key] = off.toFixed(2);
        }
        read[key] = `${grouped.toFixed(2)} to ${draining.toFixed(2)}`;
        if (grouped < 3) groupedUnder.push(key);
      }
    }
    expect(read).toEqual(HOURS_READ);
    expect(groupedUnder).toEqual(GROUPED_UNDER_THREE);
    expect(fillUnder).toEqual(LIGHT_FILL_UNDER);
  });

  // Imported light palettes whose red sits at or near 4.5 to 1, so the
  // danger text tone barely moves off it and the hours are close to the
  // fill color over itself. Each is background, foreground, then the
  // normal red and yellow slots a light theme reads.
  const IMPORTED_LIGHT: Record<string, [string, string, string, string]> = {
    'catppuccin-latte': ['#eff1f5', '#4c4f69', '#d20f39', '#df8e1d'],
    'gruvbox-light': ['#fbf1c7', '#3c3836', '#cc241d', '#d79921'],
    'selenized-light': ['#fbf3db', '#53676d', '#d2212d', '#ad8900'],
  };
  const IMPORTED_READ: Record<string, string> = {
    'catppuccin-latte': 'red 3.20, 2.98 at 28, yellow 3.30',
    'gruvbox-light': 'red 3.32, 3.11 at 28, yellow 3.35',
    'selenized-light': 'red 3.25, 3.04 at 28, yellow 3.38',
  };

  it('reads the hours at 3 to 1 or better over the fill on imported light palettes', () => {
    const base = BUILTIN_THEMES.find((theme) => theme.id === 'rubric')!.xterm;
    const read: Record<string, string> = {};
    for (const [name, [background, foreground, red, yellow]] of Object.entries(IMPORTED_LIGHT)) {
      const t = deriveChrome({ ...base, background, foreground, cursor: foreground, red, yellow });
      expect(t.appearance, name).toBe('light');
      const [panel, danger, dangerText, warn, warnText] = [
        t.panel,
        t.danger,
        t.dangerText,
        t.warn,
        t.warnText,
      ].map((value) => parseHex(value)!);
      expect(contrast(danger, panel), name).toBeGreaterThan(4.3);
      const hours = (color: typeof panel, text: typeof panel, fill: number) =>
        contrast(text, composite(color, panel, fill));
      expect(hours(danger, dangerText, 0.24), name).toBeGreaterThanOrEqual(3);
      expect(hours(warn, warnText, 0.3), name).toBeGreaterThanOrEqual(3);
      read[name] =
        `red ${hours(danger, dangerText, 0.24).toFixed(2)}, ` +
        `${hours(danger, dangerText, 0.28).toFixed(2)} at 28, ` +
        `yellow ${hours(warn, warnText, 0.3).toFixed(2)}`;
    }
    expect(read).toEqual(IMPORTED_READ);
  });
});

describe('ChipsView at your panel size', () => {
  const at = (size: number) =>
    renderToStaticMarkup(
      <PaneTextSizeContext.Provider value={size}>
        <PaneLeafContext.Provider value={LEAF}>
          <ChipsView
            current={FOURTEEN}
            tracked={TRACKED}
            hidden={false}
            box={BOARD_BOX}
            full={FULL}
            measure={FIXED_MEASURE}
          />
        </PaneLeafContext.Provider>
      </PaneTextSizeContext.Provider>,
    );
  const tops = (html: string) => [...html.matchAll(/style="top:(\d+)px"/g)].map((m) => +m[1]);

  it('draws the board exactly as before at 12 px', () => {
    expect(at(12)).toBe(draw(FOURTEEN));
    expect(tops(at(12))).toEqual([4, 32, 56, 84, 108, 132]);
    expect(at(12)).toContain('height="18.5"');
  });

  it('stacks 27 px lines further apart at 16 px, and rings a missing chip at that height', () => {
    // The lines start 5 down, and the last no longer fits the body and
    // starts the next page.
    expect(tops(at(16))).toEqual([5, 43, 75, 113, 145, 5]);
    expect(at(16)).toContain('height="25.5"');
  });
});
