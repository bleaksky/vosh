import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import panelCss from '../../styles/panel.css?raw';
import type { AffectInput, TrackedInput } from '../../lib/affectsView';
import { composite, contrast, parseHex } from '../../lib/color';
import { BUILTIN_THEMES, themeTokens } from '../../lib/themes';
import type { PaneLeaf } from '../../lib/paneLayout';
import { ChipsView, type ChipFill } from './AffectsChips';
import { chipDots, chipDotsPath, chipWidth, FIXED_MEASURE } from './chipsGrid';
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
    /<li class="pane-chip ([^"]*)"(?: style="--gauge:([^"]*)")?><span class="pane-chip-name">([^<]*)(?:<span class="pane-sr">([^<]*)<\/span>)?<\/span>(?:<span class="pane-chip-hours([^"]*)"[^>]*>([^<]*)<\/span>)?(?:<svg class="pane-chip-dots"[^>]*>.*?<\/svg>)?<\/li>/g;
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
    const chips = panelCss.slice(
      panelCss.indexOf('/* Board Affects C'),
      panelCss.indexOf('/* ── Map'),
    );
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
    const drainAt = panelCss.indexOf("[data-chip-fill='drain']");
    expect(drainAt).toBeGreaterThan(panelCss.indexOf('.pane-chip-tracked.is-danger.is-draining {'));
    const drainCss = panelCss.slice(drainAt, panelCss.indexOf('/* ── Map'));
    for (const m of drainCss.matchAll(/\[data-chip-fill='drain'\] ([^,{]+)/g)) {
      expect(m[1].trim()).toMatch(/^\.pane-chip-tracked\.is-(warn|danger)(\.is-draining)?$/);
    }
  });

  // The hours a chip running out shows over the 30 percent fill, on
  // every built in theme: the warn tone in yellow, the danger text tone
  // in bold red, and the name in the text tone. The light themes draw
  // their warn tone at 3 to 4.3 to 1 on the bare panel, so any wash
  // takes the yellow hours under 3. They stay where Grouped chips
  // already draws them. These are flagged for review, and the 30
  // percent stays as the sheet drew it.
  const UNDER_THREE = [
    'vellum warn',
    'solarized-light warn',
    'solarized-light danger',
    'everforest-light warn',
  ];

  it('keeps the hours as readable as Grouped chips on every built in theme', () => {
    const under: string[] = [];
    for (const theme of BUILTIN_THEMES) {
      const t = themeTokens(theme);
      const hex = (value: string) => {
        const rgb = parseHex(value);
        expect(rgb, `${theme.id} ${value}`).not.toBeNull();
        return rgb!;
      };
      const panel = hex(t.panel);
      const text = hex(t.text);
      for (const [tone, color, hours, shipped] of [
        // Grouped chips: 14 percent over the chip, then 20 over that.
        ['warn', hex(t.warn), hex(t.warn), [0.14, 0.2]],
        // Grouped chips: 16 percent over the chip, then 22 over that.
        ['danger', hex(t.danger), hex(t.dangerText), [0.16, 0.22]],
      ] as const) {
        const drained = composite(color, panel, 0.3);
        const tinted = composite(color, composite(color, panel, shipped[0]), shipped[1]);
        const read = contrast(hours, drained);
        // Never harder to read than Grouped chips draws the same chip.
        expect(read, `${theme.id} ${tone}`).toBeGreaterThanOrEqual(contrast(hours, tinted) - 0.01);
        // The name stays clear of the fill.
        expect(contrast(text, drained), `${theme.id} ${tone} name`).toBeGreaterThanOrEqual(3);
        if (read < 3) under.push(`${theme.id} ${tone}`);
      }
    }
    expect(under).toEqual(UNDER_THREE);
  });
});
