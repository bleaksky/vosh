import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import affectsCss from '../../styles/affects.css?raw';
import type { PaneLeaf } from '../paneLayout';
import { groupCurrentAffects, type CurrentAffect } from '../../stores/gmcp/affectsStore';
import type { TrackedAffect } from '../../ipc/affects';
import { aabahranPacket } from '../../test/aabahranGmcp';
import { TimersView } from './AffectsTimers';
import { PaneLeafContext } from '../paneActions';
import { PaneTextSizeContext } from '../paneTextSize';

// The stores behind AffectsPane reach the Tauri bridge. TimersView,
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
      <TimersView current={current} tracked={tracked} hidden={hidden} box={box} />
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

// Ilsabet on the approved board: his eight tracked affects in his order,
// and what the game sends while bless has worn off.
const ILSABET_TRACKED: TrackedAffect[] = [
  'mounted',
  'sanctuary',
  'bless',
  'armor',
  'shield',
  'stone skin',
  'fly',
  'levitate',
].map((name) => ({ name, label: null }));
const ILSABET: CurrentAffect[] = [
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
    /<li class="pane-affect pane-affect-(\w+)[^"]*"[^>]*>(?:<span class="pane-affect-mark is-(\w+)"[^>]*><\/span>)?<span class="pane-affect-hours([^"]*)"[^>]*>([^<]*)<\/span><span class="pane-affect-name">([^<]*)<span class="visually-hidden">([^<]*)<\/span>/g;
  for (const m of html.matchAll(cell)) {
    const tone = m[3].trim().replace('is-', '');
    out.push([m[2] ?? '.', `${m[4]}${tone ? `(${tone})` : ''}`, m[5], m[6]].join(' ').trim());
  }
  return out;
}

/** The declarations of one rule in affects.css. */
function rule(selector: string): string {
  const at = affectsCss.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return affectsCss.slice(at, affectsCss.indexOf('}', at));
}

const list = (name: string) => groupCurrentAffects(aabahranPacket(name).data);

describe('TimersView', () => {
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
    const html = draw(ILSABET, false, ILSABET_TRACKED, BOARD_BOX);
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
      ...ILSABET,
      affect('faerie fire', 3),
      affect('protective shield', 6),
      affect('frenzy', 9),
      affect('giant strength', 40),
      affect('detect magic', 45),
    ];
    const html = draw(fight, false, ILSABET_TRACKED, BOARD_BOX);
    const cells = cellsOf(html);
    expect(cells[8]).toBe('harmful 3 faerie fire , 3 hours, harmful');
    expect(html).toContain('aria-label="5 more affects, scroll to them"');
    expect(html).toContain('>5 more</button>');
    // Every affect stays in the list for a screen reader.
    expect(cells).toHaveLength(20);
  });

  it('widens the hours column for a four digit duration', () => {
    // The inner calm psalm lasts 1200 hours.
    const calm = draw([...ILSABET, affect('inner calm', 1200)], false, ILSABET_TRACKED, BOARD_BOX);
    expect(calm).toContain('<div class="pane-body" style="--affect-hours-ch:4">');
    expect(cellsOf(calm)).toContain('. 1200 inner calm , 1200 hours');
    expect(draw(ILSABET, false, ILSABET_TRACKED, BOARD_BOX)).toContain(
      '<div class="pane-body" style="--affect-hours-ch:3">',
    );
    // Every cell's hours column takes the widest count, so the names
    // stay in line and never run into the digits.
    const hours = rule('.pane-affect-hours');
    expect(hours).toContain('width: var(--mud-affects-hours)');
    expect(hours).toContain('min-width: calc(var(--affect-hours-ch, 3) * 1ch)');
    expect(hours).toContain('flex: none');
  });

  it('lines the count up with the names over the same hours column', () => {
    const fight = [
      ...ILSABET,
      affect('faerie fire', 3),
      affect('protective shield', 6),
      affect('frenzy', 9),
      affect('giant strength', 40),
      affect('detect magic', 45),
    ];
    const html = draw(fight, false, ILSABET_TRACKED, BOARD_BOX);
    expect(html).toMatch(
      /<li class="pane-affects-more-cell"[^>]*><span class="pane-affect-hours" aria-hidden="true"><\/span><button type="button" class="pane-affects-more"/,
    );
    expect(rule('.pane-affects-more')).toContain('margin: 0 0 0 8px');
  });

  it('shows your label in place of the name, exactly as you wrote it', () => {
    const html = draw([affect('sanctuary', 9)], false, [{ name: 'sanctuary', label: 'sanc' }]);
    expect(cellsOf(html)).toEqual(['up 9 sanc , 9 hours']);
  });

  it('keeps the dot on a body with no marker attribute, as before the choice', () => {
    const html = draw(ILSABET, false, ILSABET_TRACKED, BOARD_BOX);
    expect(html).not.toContain('data-affects-marker');
    const dot = renderToStaticMarkup(
      <PaneLeafContext.Provider value={LEAF}>
        <TimersView
          current={ILSABET}
          tracked={ILSABET_TRACKED}
          hidden={false}
          box={BOARD_BOX}
          marker="dot"
        />
      </PaneLeafContext.Provider>,
    );
    expect(dot).toBe(html);
  });

  it('names the marker on the body, and every cell keeps its state mark', () => {
    for (const marker of ['square', 'plus_minus', 'none'] as const) {
      const html = renderToStaticMarkup(
        <PaneLeafContext.Provider value={LEAF}>
          <TimersView
            current={ILSABET}
            tracked={ILSABET_TRACKED}
            hidden={false}
            box={BOARD_BOX}
            marker={marker}
          />
        </PaneLeafContext.Provider>,
      );
      expect(html).toContain(
        `<div class="pane-body" style="--affect-hours-ch:3" data-affects-marker="${marker}">`,
      );
      // The state still rides on each mark, so color carries it in
      // every shape.
      expect(cellsOf(html).slice(0, 3)).toEqual([
        'up + mounted , permanent',
        'danger 1(danger) sanctuary , 1 hour, running out',
        'missing - bless , missing',
      ]);
    }
  });

  it('paints every marker shape in the color of its state', () => {
    expect(rule('.pane-affect-mark')).toContain('--mark: var(--success)');
    expect(rule('.pane-affect-mark.is-up')).toContain('background: var(--mark)');
    expect(rule('.pane-affect-mark.is-warn')).toContain('--mark: var(--warn)');
    expect(rule('.pane-affect-mark.is-danger')).toContain('--mark: var(--danger)');
    // The ring keeps its 1.25 px width.
    const missing = rule('.pane-affect-mark.is-missing');
    expect(missing).toContain('--mark: var(--danger)');
    expect(missing).toContain('border: 1.25px solid var(--mark)');
    expect(rule("[data-affects-marker='square'] .pane-affect-mark:not(.is-harmful)")).toContain(
      'border-radius: 1.5px',
    );
    expect(
      rule("[data-affects-marker='plus_minus'] .pane-affect-mark:not(.is-harmful)::before"),
    ).toContain('height: 1.5px');
    // A plus while you have it: the upright stroke, which a minus drops.
    expect(affectsCss).toMatch(
      /\.pane-affect-mark:not\(\.is-harmful\):not\(\.is-missing\)::after \{\s*left: 3\.25px;\s*top: 0;\s*width: 1\.5px;\s*height: 8px;/,
    );
    // No mark sets the hours at the text edge.
    expect(rule("[data-affects-marker='none'] .pane-affect-mark")).toContain('display: none');
    expect(rule("[data-affects-marker='none'] .pane-affect-hours")).toContain('margin-left: 0');
    // The marker rules come after the base rules they refine.
    expect(affectsCss.indexOf("[data-affects-marker='none'] .pane-affect-hours {")).toBeGreaterThan(
      affectsCss.indexOf('.pane-affect-hours {'),
    );
  });

  it('marks the body for the recast tint only while it is on', () => {
    const drawTint = (tint: boolean) =>
      renderToStaticMarkup(
        <PaneLeafContext.Provider value={LEAF}>
          <TimersView
            current={ILSABET}
            tracked={ILSABET_TRACKED}
            hidden={false}
            box={BOARD_BOX}
            tint={tint}
          />
        </PaneLeafContext.Provider>,
      );
    expect(drawTint(false)).toBe(draw(ILSABET, false, ILSABET_TRACKED, BOARD_BOX));
    expect(drawTint(true)).toContain(
      '<div class="pane-body" style="--affect-hours-ch:3" data-affects-tint="">',
    );
  });

  it('washes a missing row red and one about to drop in the tone of its hours', () => {
    // The selector wraps after [data-affects-tint].
    const wash = rule(
      ':is(.pane-affect, .pane-countdown-cell):is(.pane-affect-missing, .pane-affect-expiring)::before',
    );
    expect(wash).toContain('left: -6px');
    expect(wash).toContain('right: -6px');
    expect(wash).toContain('top: 1px');
    expect(wash).toContain('z-index: -1');
    expect(wash).toContain('background: var(--recast-wash)');
    // Missing at 8 percent keeps a red name at 4 to 1 or better on the
    // light themes.
    expect(rule('[data-affects-tint] .pane-affect-missing')).toContain('var(--danger) 8%');
    // Running out takes board C's own tints.
    expect(rule('[data-affects-tint] .pane-affect-expiring')).toContain('var(--warn) 14%');
    expect(rule('[data-affects-tint] .pane-affect-expiring:has(.is-danger)')).toContain(
      'var(--danger) 16%',
    );
  });

  // Exactly at running out and at almost gone, an affect at none, a
  // permanent one, and one you track that is up for a while.
  const EDGES: CurrentAffect[] = [
    affect('sanctuary', 5),
    affect('armor', 6),
    affect('shield', 2),
    affect('fly', 0),
    affect('mounted', -1),
    affect('haste', 4),
  ];
  const EDGES_TRACKED: TrackedAffect[] = [
    'sanctuary',
    'bless',
    'armor',
    'shield',
    'fly',
    'mounted',
  ].map((name) => ({ name, label: null }));
  const drawAt = (thresholds?: { runningOut: number; almostGone: number }) =>
    renderToStaticMarkup(
      <PaneLeafContext.Provider value={LEAF}>
        <TimersView
          current={EDGES}
          tracked={EDGES_TRACKED}
          hidden={false}
          box={BOARD_BOX}
          tint
          thresholds={thresholds}
        />
      </PaneLeafContext.Provider>,
    );

  it('colors the hours, the marks, the counts and the tint by the hours you set', () => {
    const html = drawAt({ runningOut: 5, almostGone: 2 });
    expect(html).toContain('<span class="pane-meta pane-meta-danger">1 missing</span>');
    expect(html).toContain('<span class="pane-meta pane-meta-warn">3 running out</span>');
    expect(cellsOf(html)).toEqual([
      'warn 5(warn) sanctuary , 5 hours, running out',
      'missing - bless , missing',
      'up 6 armor , 6 hours',
      'danger 2(danger) shield , 2 hours, running out',
      'danger 0(danger) fly , 0 hours, running out',
      'up + mounted , permanent',
      // Not tracked: the hours alone take the color.
      '. 4(warn) haste , 4 hours',
    ]);
    // The rows running out carry the class the tint washes.
    expect(html).toContain(
      '<li class="pane-affect pane-affect-expiring"><span class="pane-affect-mark is-warn"',
    );
    expect(html.match(/pane-affect-expiring/g)).toHaveLength(3);
  });

  it('draws the same rows as before with the hours left at two and one', () => {
    const html = drawAt();
    expect(drawAt({ runningOut: 2, almostGone: 1 })).toBe(html);
    expect(html).toContain('<span class="pane-meta pane-meta-warn">2 running out</span>');
    expect(cellsOf(html)).toEqual([
      'up 5 sanctuary , 5 hours',
      'missing - bless , missing',
      'up 6 armor , 6 hours',
      'warn 2(warn) shield , 2 hours, running out',
      'danger 0(danger) fly , 0 hours, running out',
      'up + mounted , permanent',
      '. 4 haste , 4 hours',
    ]);
  });

  it('leaves no yellow stage when both hours are equal', () => {
    const cells = cellsOf(drawAt({ runningOut: 4, almostGone: 4 }));
    expect(cells[3]).toBe('danger 2(danger) shield , 2 hours, running out');
    expect(cells[6]).toBe('. 4(danger) haste , 4 hours');
    expect(cells[0]).toBe('up 5 sanctuary , 5 hours');
  });

  it('sets the names and the hours at your panel size in the game face', () => {
    // The game face is your terminal font under As designed and your
    // Panel font under any other pick (src/panel/panelFont.test.ts).
    expect(rule('.pane-affect-name')).toContain('font-family: var(--font-panel-game);');
    expect(rule('.pane-affect-hours')).toContain('font-family: var(--font-panel-game);');
    expect(rule('.pane-affect-name')).toContain('font-size: var(--mud-text)');
    expect(rule('.pane-affect-hours')).toContain('font-size: var(--mud-text)');
    expect(rule('.pane-affect-hours')).toContain('text-align: right');
    expect(rule('.pane-affect-hours.is-danger')).toContain('font-weight: 700');
  });
});

describe('TimersView at your terminal size', () => {
  const at = (size: number) =>
    renderToStaticMarkup(
      <PaneTextSizeContext.Provider value={size}>
        <PaneLeafContext.Provider value={LEAF}>
          <TimersView
            current={ILSABET}
            tracked={ILSABET_TRACKED}
            hidden={false}
            box={{ width: 494, height: 191 }}
          />
        </PaneLeafContext.Provider>
      </PaneTextSizeContext.Provider>,
    );

  it('draws the board exactly as before at 12 px', () => {
    const html = at(12);
    expect(html).toBe(draw(ILSABET, false, ILSABET_TRACKED, { width: 494, height: 191 }));
    expect(html).toContain('style="height:88px"');
    expect(html).toContain('grid-template-rows:repeat(4, 22px)');
  });

  it('pages the rest on 29 px rows at 16 px', () => {
    // Four rows of slots and the rule leave two rows of the rest.
    const html = at(16);
    expect(html).toContain('style="height:58px"');
    expect(html).toMatch(/grid-template-rows:repeat\(\d+, 29px\)/);
  });
});
