import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import affectsCss from '../../styles/affects.css?raw';
import type { AffectInput, TrackedInput } from './affectsView';
import type { PaneLeaf } from '../paneLayout';
import type { AffectsMarker } from '../../ipc/affects';
import { CountdownView } from './AffectsCountdown';
import { PaneLeafContext } from '../paneActions';
import { PaneTextSizeContext } from '../paneTextSize';

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
    thresholds?: { runningOut: number; almostGone: number };
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
        thresholds={options.thresholds}
      />
    </PaneLeafContext.Provider>,
  );
}

/** Every cell as `mark name hours(tone) meter% words`, in order. */
function cellsOf(html: string): string[] {
  const out: string[] = [];
  const cell =
    /<li class="pane-countdown-cell pane-affect-(\w+)[^"]*"[^>]*>(?:<span class="pane-affect-mark is-(\w+)"[^>]*><\/span>)?<span class="pane-countdown-line"><span class="pane-countdown-name">([^<]*)(?:<span class="visually-hidden">([^<]*)<\/span>)?<\/span><span class="pane-countdown-hours([^"]*)"[^>]*>([^<]*)<\/span><\/span>(?:<span class="pane-countdown-meter"[^>]*><span class="pane-countdown-fill" style="width:([^%]*)%"><\/span><\/span>)?<\/li>/g;
  for (const m of html.matchAll(cell)) {
    const tone = m[5].trim().replace('is-', '');
    const meter = m[7] === undefined ? 'no meter' : `${m[7]}%`;
    out.push(
      [m[2] ?? '.', m[3], `${m[6]}${tone ? `(${tone})` : ''}`, meter, m[4] ?? ''].join(' ').trim(),
    );
  }
  return out;
}

/** A selector's specificity as [ids, classes, types]. `:is()`, `:not()`
 *  and `:has()` count their most specific argument, `:where()` none. */
function specificity(selector: string): number[] {
  const out = [0, 0, 0];
  const ident = (at: number) => {
    let j = at;
    while (j < selector.length && /[\w-]/.test(selector[j])) j += 1;
    return j;
  };
  let i = 0;
  while (i < selector.length) {
    const ch = selector[i];
    if (ch === '#') {
      out[0] += 1;
      i = ident(i + 1);
    } else if (ch === '.') {
      out[1] += 1;
      i = ident(i + 1);
    } else if (ch === '[') {
      out[1] += 1;
      i = selector.indexOf(']', i) + 1;
    } else if (ch === ':' && selector[i + 1] === ':') {
      out[2] += 1;
      i = ident(i + 2);
    } else if (ch === ':') {
      const end = ident(i + 1);
      const name = selector.slice(i + 1, end);
      i = end;
      if (selector[i] !== '(') {
        out[1] += 1;
        continue;
      }
      let depth = 0;
      let close = i;
      for (; close < selector.length; close += 1) {
        if (selector[close] === '(') depth += 1;
        if (selector[close] === ')' && --depth === 0) break;
      }
      const args = selector.slice(i + 1, close).split(',');
      i = close + 1;
      if (name === 'where') continue;
      if (!['is', 'not', 'has'].includes(name)) {
        out[1] += 1;
        continue;
      }
      const best = args.map(specificity).sort(compare).at(-1) ?? [0, 0, 0];
      best.forEach((n, k) => (out[k] += n));
    } else if (/[a-z]/i.test(ch)) {
      out[2] += 1;
      i = ident(i);
    } else {
      i += 1;
    }
  }
  return out;
}

const compare = (x: number[], y: number[]) => x[0] - y[0] || x[1] - y[1] || x[2] - y[2];

/** The top level rules of affects.css that set `property`, each selector
 *  on its own with where the rule sits. */
function rulesSetting(property: string): { selector: string; value: string; at: number }[] {
  const css = affectsCss.replace(/\/\*[\s\S]*?\*\//g, '');
  const out: { selector: string; value: string; at: number }[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < css.length; i += 1) {
    if (css[i] === '{') {
      if (depth === 0) {
        const selectors = css.slice(start, i).trim();
        const body = css.slice(i + 1, css.indexOf('}', i));
        const value = new RegExp(`(?:^|[;\\s])${property}:\\s*([^;]+);`).exec(body)?.[1];
        if (!selectors.startsWith('@') && value !== undefined) {
          for (const selector of selectors.split(/,(?![^(]*\))/)) {
            out.push({ selector: selector.replace(/\s+/g, ' ').trim(), value, at: i });
          }
        }
      }
      depth += 1;
    } else if (css[i] === '}') {
      depth -= 1;
      if (depth === 0) start = i + 1;
    }
  }
  return out;
}

/** The declarations of one rule in affects.css. */
function rule(selector: string): string {
  const at = affectsCss.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return affectsCss.slice(at, affectsCss.indexOf('}', at));
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

  it('colors the hours, the marks, the meter and the counts by the hours you set', () => {
    const edges = [
      aff('sanctuary', 5),
      aff('armor', 6),
      aff('shield', 2),
      aff('fly', 0),
      aff('mounted', -1),
      aff('haste', 4),
      aff('detect magic', null),
    ];
    const tracked = ['sanctuary', 'bless', 'armor', 'shield', 'fly', 'mounted'].map((name) => ({
      name,
    }));
    // Sanctuary has no full yet, the most seen is unknown, so it reads full.
    const full = { armor: 48, shield: 8, fly: 53, haste: 26 };
    const html = draw(edges, { tracked, full, thresholds: { runningOut: 5, almostGone: 2 } });
    expect(html).toContain('<span class="pane-meta pane-meta-danger">1 missing</span>');
    expect(html).toContain('<span class="pane-meta pane-meta-warn">3 running out</span>');
    expect(cellsOf(html)).toEqual([
      'missing bless - 0.0% , missing',
      'danger fly 0(danger) 0.0% , 0 hours, running out',
      'danger shield 2(danger) 25.0% , 2 hours, running out',
      '. haste 4(warn) 15.4% , 4 hours',
      // Exactly at running out.
      'warn sanctuary 5(warn) 100.0% , 5 hours, running out',
      'up armor 6 12.5% , 6 hours',
      'up mounted + 100.0% , permanent',
      '. detect magic  no meter',
    ]);
    // The meter takes the tone of the hours.
    expect(html).toContain('<li class="pane-countdown-cell pane-affect-expiring is-warn"');
    expect(html).toContain('<li class="pane-countdown-cell pane-affect-untracked is-warn"');
    // At two and one, sanctuary and haste are up again and shield only yellow.
    const before = cellsOf(draw(edges, { tracked, full }));
    expect(before).toContain('warn shield 2(warn) 25.0% , 2 hours, running out');
    expect(before).toContain('up sanctuary 5 100.0% , 5 hours');
    expect(before).toContain('. haste 4 15.4% , 4 hours');
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

  it('marks the body for the recast tint only while it is on', () => {
    const tinted = renderToStaticMarkup(
      <PaneLeafContext.Provider value={LEAF}>
        <CountdownView
          current={FOURTEEN}
          tracked={TRACKED}
          hidden={false}
          box={BOARD_BOX}
          full={FULL}
          tint
        />
      </PaneLeafContext.Provider>,
    );
    expect(tinted).toContain('<div class="pane-body" data-affects-tint="">');
    expect(draw(FOURTEEN)).not.toContain('data-affects-tint');
    // Over the text and the meter, 22 tall: of the rules that set the
    // wash's top on a Countdown cell, the one that wins says 0.
    const tops = rulesSetting('top').filter(
      (r) =>
        r.selector.startsWith('[data-affects-tint]') &&
        r.selector.includes('.pane-countdown-cell') &&
        r.selector.endsWith('::before'),
    );
    expect(tops.length).toBeGreaterThan(1);
    const wins = tops.sort(
      (x, y) => compare(specificity(x.selector), specificity(y.selector)) || x.at - y.at,
    );
    expect(wins.at(-1)?.value).toBe('0');
    expect(specificity('[data-affects-tint] .pane-countdown-cell::before')).toEqual([0, 2, 1]);
    expect(
      specificity(
        '[data-affects-tint] :is(.pane-affect, .pane-countdown-cell):is(.pane-affect-missing, .pane-affect-expiring)::before',
      ),
    ).toEqual([0, 3, 1]);
  });

  it('names the marker on the body only when it is not the dot', () => {
    expect(draw(FOURTEEN)).toContain('<div class="pane-body">');
    expect(draw(FOURTEEN, { marker: 'none' })).toContain(
      '<div class="pane-body" data-affects-marker="none">',
    );
  });

  it('sets the geometry of the board', () => {
    // The rows, the line, and the meter follow your terminal size, and
    // at 12 px read 23, 17, and 19 (paneTextSize.test.ts).
    expect(rule('.pane-countdown')).toContain('column-gap: 24px');
    expect(rule('.pane-countdown')).toContain('padding: 0 12px 0 18px');
    expect(rule('.pane-countdown-cell')).toContain('height: var(--mud-countdown-row)');
    expect(rule('.pane-countdown-cell .pane-affect-mark')).toContain(
      'top: round(1px + var(--mud-line) / 2 - 4px, 1px)',
    );
    expect(rule('.pane-countdown-line')).toContain('height: calc(var(--mud-line) + 1px)');
    expect(rule('.pane-countdown-line')).toContain('padding: 1px 0 0 14px');
    expect(rule('.pane-countdown-meter')).toContain('top: var(--mud-countdown-meter-top)');
    expect(rule('.pane-countdown-meter')).toContain('background: var(--divider)');
    expect(rule('.pane-countdown-fill')).toContain('background: var(--tertiary)');
    expect(rule('.pane-countdown-cell.is-warn .pane-countdown-fill')).toContain('var(--warn)');
    expect(rule('.pane-countdown-cell.is-danger .pane-countdown-fill')).toContain('var(--danger)');
    expect(rule('.pane-countdown-hours.is-danger')).toContain('font-weight: 700');
  });
});

describe('CountdownView at your terminal size', () => {
  const at = (size: number) =>
    renderToStaticMarkup(
      <PaneTextSizeContext.Provider value={size}>
        <PaneLeafContext.Provider value={LEAF}>
          <CountdownView
            current={TWENTY}
            tracked={TRACKED}
            hidden={false}
            box={BOARD_BOX}
            full={FULL}
          />
        </PaneLeafContext.Provider>
      </PaneTextSizeContext.Provider>,
    );

  it('draws the board exactly as before at 12 px', () => {
    expect(at(12)).toBe(draw(TWENTY));
    expect(at(12)).toContain('style="height:184px"');
    expect(at(12)).toMatch(/grid-template-rows:repeat\(\d+, 23px\)/);
  });

  it('fits six rows of 31 px in the same body at 16 px', () => {
    expect(at(16)).toContain('style="height:186px"');
    expect(at(16)).toMatch(/grid-template-rows:repeat\(\d+, 31px\)/);
  });
});
