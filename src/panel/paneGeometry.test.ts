import { describe, expect, it } from 'vitest';
import {
  affectsPaneRows,
  type AffectInput,
  type AffectRow,
  type AffectRowState,
} from './affects/affectsView';
import {
  addPane,
  paneRef,
  defaultLayout,
  PANE_TYPES,
  sanitize,
  splitPane,
  type PaneSplit,
} from './paneLayout';
import { chipGroups, chipsMinBody, FIXED_MEASURE } from './affects/chipsGrid';
import { affectHours } from './paneText';
import {
  affectsMinH,
  affectsMinIn,
  affectsStyleMinH,
  allocate,
  chipsMinH,
  countdownMinH,
  distribute,
  dragSizes,
  fitsPanel,
  groupMinH,
  layoutPanes,
  minExtent,
  PANE_FLOOR_H,
  PANE_MIN_H,
  paneFloorH,
  paneMinH,
  paneWidth,
} from './paneGeometry';

const total = (xs: number[]) => xs.reduce((a, b) => a + b, 0);

describe('distribute', () => {
  it('splits into whole pixels that sum to the total', () => {
    const sizes = distribute(663, [0.6, 0.4]);
    expect(sizes).toEqual([398, 265]);
    expect(sizes.reduce((a, b) => a + b, 0)).toBe(663);
  });

  it('keeps every share within a pixel across many siblings', () => {
    const sizes = distribute(100, [1, 1, 1]);
    expect(sizes.reduce((a, b) => a + b, 0)).toBe(100);
    for (const s of sizes) expect(Math.abs(s - 100 / 3)).toBeLessThan(1);
  });

  it('shares evenly when no weight is positive', () => {
    expect(distribute(90, [0, 0, 0])).toEqual([30, 30, 30]);
  });

  it('returns nothing for no children and zero for no space', () => {
    expect(distribute(100, [])).toEqual([]);
    expect(distribute(-5, [1, 1])).toEqual([0, 0]);
  });
});

describe('PANE_MIN_H', () => {
  it('reads each pane type at its header plus its rows, or a set height', () => {
    // 28 px header, 22 px rows.
    expect(PANE_MIN_H).toEqual({
      map: 180,
      affects: 160,
      group: 94,
      chat: 120,
      imm: 94,
      lua: 94,
    });
    expect(PANE_FLOOR_H).toBe(50);
  });
});

// Affects rows by state, in the order the pane draws them.
const rowsOf = (...states: AffectRowState[]): AffectRow[] =>
  states.map((state, i) => ({ key: `a${i}`, name: `a${i}`, state, ticks: null, tone: null }));
const times = (n: number, state: AffectRowState): AffectRowState[] =>
  Array<AffectRowState>(n).fill(state);

// The approved board's Affects pane: eight tracked slots, bless among
// them missing, then seven more under the hairline.
const BOARD_AFFECTS = rowsOf(
  'present',
  'expiring',
  'missing',
  'present',
  'present',
  'present',
  'expiring',
  'present',
  ...times(7, 'untracked'),
);

// The same slots in a fight: four harmful affects and three more.
const FIGHT_AFFECTS = rowsOf(
  ...times(8, 'present'),
  ...times(4, 'harmful'),
  ...times(3, 'untracked'),
);

describe('affectsMinH', () => {
  it('keeps the stock minimum when the slots and one row of the rest fit in it', () => {
    // Four rows of slots, the hairline, and a row of the rest: 147 px.
    expect(affectsMinH(BOARD_AFFECTS)).toBe(PANE_MIN_H.affects);
    expect(affectsMinH([])).toBe(PANE_MIN_H.affects);
  });

  it('holds every tracked slot', () => {
    expect(affectsMinH(rowsOf(...times(16, 'present')))).toBe(28 + 8 * 22);
    expect(affectsMinH(rowsOf(...times(15, 'missing'), 'untracked'))).toBe(28 + 9 * 22 + 9);
  });

  it('holds every harmful affect and the count after them', () => {
    // Four rows of slots, the hairline, then three rows for the four
    // harmful affects and the count cell.
    expect(affectsMinH(FIGHT_AFFECTS)).toBe(28 + 7 * 22 + 9);
  });

  it('puts harmful affects at the top when you track nothing', () => {
    expect(affectsMinH(rowsOf('harmful', 'harmful', 'untracked'))).toBe(PANE_MIN_H.affects);
    expect(affectsMinH(rowsOf(...times(13, 'harmful')))).toBe(28 + 7 * 22);
  });

  it('counts rows for one column in a narrow pane', () => {
    expect(affectsMinH(rowsOf(...times(8, 'present'), 'harmful', 'untracked'), 1)).toBe(
      28 + 10 * 22 + 9,
    );
    // One affect and the count need a row each in one column.
    expect(affectsMinH(BOARD_AFFECTS, 1)).toBe(28 + 10 * 22 + 9);
    expect(affectsMinH(rowsOf(...times(8, 'present'), 'untracked'), 1)).toBe(28 + 9 * 22 + 9);
  });

  it('stops at a dozen rows and the hairline', () => {
    const long = rowsOf(...times(30, 'present'), 'harmful');
    expect(affectsMinH(long)).toBe(28 + 12 * 22 + 9);
  });
});

describe('affectsMinIn', () => {
  // Ilsabet's eight slots and twelve more, faerie fire harmful among them.
  const twenty = rowsOf(...times(8, 'present'), 'harmful', ...times(11, 'untracked'));

  it('counts two columns for the Affects pane across the whole panel', () => {
    expect(affectsMinIn(defaultLayout().root, 494, twenty)).toBe(affectsMinH(twenty, 2));
  });

  it('counts one column once Split right leaves the pane narrow', () => {
    // The pane draws one column at 247 px, so its minimum holds all
    // eight slots, the hairline, faerie fire, and the count.
    const tree = splitPane(defaultLayout().root, 'affects', 'row', paneRef('group'));
    expect(affectsMinIn(tree, 494, twenty)).toBe(28 + 8 * 22 + 9 + 2 * 22);
    const { leaves } = layoutPanes(tree, 494, 664, { affects: affectsMinIn(tree, 494, twenty) });
    const affects = leaves.find((l) => l.leaf.pane === 'affects');
    expect(affects?.rect.w).toBe(247);
    expect(affects?.rect.h).toBeGreaterThanOrEqual(257);
  });

  it('reads the panel width when the tree has no Affects pane', () => {
    const tree = sanitize({ id: 'root', split: 'column', children: [{ pane: 'map' }] });
    expect(affectsMinIn(tree, 300, twenty)).toBe(affectsMinH(twenty, 1));
  });
});

// The boards' scenes by what the game sends, Ilsabet's eight tracked.
const tracked = [
  'mounted',
  'sanctuary',
  'bless',
  'armor',
  'shield',
  'stone skin',
  'fly',
  'levitate',
].map((name) => ({ name }));
const aff = (name: string, duration: number): AffectInput => ({ name, duration });
const five = [
  aff('sanctuary', 9),
  aff('armor', 44),
  aff('shield', 44),
  aff('fly', 50),
  aff('mounted', -1),
];
const fourteen = [
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
const twenty = [
  ...fourteen,
  aff('frenzy', 9),
  aff('protective shield', 6),
  aff('giant strength', 40),
  aff('detect magic', 45),
  aff('faerie fire', 3),
];
const thirty = [
  ...twenty,
  aff('infravision', 50),
  aff('barkskin', 30),
  aff('regeneration', 25),
  aff('camouflage', 12),
  aff('steel wall', 16),
  aff('protection evil', 24),
  aff('holy touch', 33),
  aff('Battle Hymn', 7),
  aff('poison', 4),
  aff('detect hidden', 50),
];
const rows = (list: AffectInput[]) => affectsPaneRows(list, tracked, false);

describe('countdownMinH', () => {
  it('holds every row that asks something of you, as SPEC 1.8 counts it', () => {
    const table = [
      [five, 2, 160],
      [five, 1, 160],
      [fourteen, 2, 160],
      [fourteen, 1, 160],
      [twenty, 2, 160],
      [twenty, 1, 160],
      [thirty, 2, 160],
      // Down to poison and the count: six rows of 23 px.
      [thirty, 1, 28 + 6 * 23],
    ] as const;
    for (const [list, columns, want] of table) {
      expect(countdownMinH(rows([...list]), columns), `${list.length} in ${columns}`).toBe(want);
    }
  });

  it('stops at a dozen rows', () => {
    const lost = affectsPaneRows(
      [],
      Array.from({ length: 30 }, (_, i) => ({ name: `slot ${i}` })),
      false,
    );
    expect(countdownMinH(lost, 1)).toBe(28 + 12 * 23);
  });

  it('follows the style the pane draws', () => {
    const tree = splitPane(defaultLayout().root, 'affects', 'row', paneRef('group'));
    expect(affectsMinIn(tree, 494, rows(thirty), 'countdown')).toBe(28 + 6 * 23);
    expect(affectsMinIn(tree, 494, rows(thirty), 'timers')).toBe(affectsMinH(rows(thirty), 1));
  });
});

describe('chipsMinH', () => {
  it('holds every Recast, Tracked, and harmful chip, as SPEC 1.8 counts it', () => {
    const table = [
      [five, 494, 160],
      [five, 247, 160],
      [fourteen, 494, 160],
      [fourteen, 247, 160],
      [twenty, 494, 160],
      // Down to faerie fire run in after Other, and the count.
      [twenty, 247, 180],
      [thirty, 494, 160],
      // Poison wraps under faerie fire.
      [thirty, 247, 204],
    ] as const;
    for (const [list, width, want] of table) {
      expect(chipsMinH(rows([...list]), width), `${list.length} at ${width}`).toBe(want);
    }
  });

  it('follows the style the pane draws, packed to its own width', () => {
    const tree = splitPane(defaultLayout().root, 'affects', 'row', paneRef('group'));
    expect(affectsMinIn(tree, 494, rows(thirty), 'chips')).toBe(204);
    expect(affectsMinIn(defaultLayout().root, 494, rows(thirty), 'chips')).toBe(160);
    // Draining chips packs the same chips, so it holds the same lines.
    expect(affectsMinIn(tree, 494, rows(thirty), 'chips_drain')).toBe(204);
    expect(affectsMinIn(defaultLayout().root, 494, rows(thirty), 'chips_drain')).toBe(160);
  });
});

describe('paneWidth', () => {
  it('reads the width a pane gets, whatever the heights', () => {
    const tree = splitPane(defaultLayout().root, 'affects', 'row', paneRef('group'));
    expect(paneWidth(tree, 494, 'affects')).toBe(247);
    expect(paneWidth(tree, 494, 'group')).toBe(246);
    expect(paneWidth(defaultLayout().root, 494, 'affects')).toBe(494);
    expect(paneWidth(defaultLayout().root, 494, 'chat')).toBeNull();
  });
});

describe('groupMinH', () => {
  it('holds every member up to six, then peeks at the seventh', () => {
    expect(groupMinH(0)).toBe(PANE_MIN_H.group);
    expect(groupMinH(3)).toBe(PANE_MIN_H.group);
    expect(groupMinH(4)).toBe(28 + 4 * 22);
    expect(groupMinH(6)).toBe(28 + 6 * 22);
    expect(groupMinH(12)).toBe(28 + 6 * 22 + 11);
  });
});

describe('minExtent', () => {
  it('adds up a stack and takes the tallest of a row', () => {
    const tree = splitPane(defaultLayout().root, 'affects', 'row', paneRef('group'));
    // map, then affects beside group, with a 1 px handle between.
    expect(minExtent(tree, 'column')).toBe(180 + 1 + 160);
    expect(minExtent(tree, 'row')).toBe(120 + 1 + 120);
    expect(minExtent(tree, 'column', true)).toBe(50 + 1 + 50);
  });

  it('needs nothing for an empty panel', () => {
    const empty: PaneSplit = { id: 'root', split: 'column', weight: 1, children: [] };
    expect(minExtent(empty, 'column')).toBe(0);
    expect(fitsPanel(empty, 0, 0)).toBe(true);
  });
});

describe('fitsPanel', () => {
  it('holds a panel that has room for every minimum', () => {
    const root = defaultLayout().root;
    expect(fitsPanel(root, 300, 341)).toBe(true);
    expect(fitsPanel(root, 300, 340)).toBe(false);
  });

  it('refuses a third pane side by side in the stock width', () => {
    const two = splitPane(defaultLayout().root, 'affects', 'row', paneRef('group'));
    expect(fitsPanel(two, 300, 664)).toBe(true);
    const three = splitPane(two, 'group', 'row', paneRef('chat'));
    expect(fitsPanel(three, 300, 664)).toBe(false);
    expect(fitsPanel(three, 362, 664)).toBe(true);
  });
});

describe('allocate', () => {
  it('shares by weight when every share clears its minimum', () => {
    expect(allocate(663, [0.6, 0.4], [180, 160])).toEqual(distribute(663, [0.6, 0.4]));
  });

  it('lifts a short share to its minimum and gives the rest by weight', () => {
    expect(allocate(400, [0.9, 0.1], [180, 160])).toEqual([240, 160]);
    const sizes = allocate(500, [0.8, 0.1, 0.1], [180, 94, 120]);
    expect(sizes).toEqual([286, 94, 120]);
    expect(total(sizes)).toBe(500);
  });

  it('keeps every pane at its minimum or more across many heights', () => {
    const mins = [180, 94, 160, 120, 94];
    const weights = [0.45, 0.125, 0.3, 0.1, 0.025];
    for (let h = total(mins); h < 1400; h += 7) {
      const sizes = allocate(h, weights, mins);
      expect(total(sizes)).toBe(h);
      sizes.forEach((s, i) => expect(s).toBeGreaterThanOrEqual(mins[i]));
    }
  });

  it('squeezes the lightest panes first when the minimums do not fit', () => {
    // Floors of 50 each, then the heaviest (affects) takes its 160,
    // then group, and the map, lightest, keeps its floor.
    expect(allocate(300, [0.2, 0.5, 0.3], [180, 160, 94], [50, 50, 50])).toEqual([50, 160, 90]);
    // Map over affects on a short panel: the map first.
    expect(allocate(299, [0.6, 0.4], [180, 160], [50, 50])).toEqual([180, 119]);
  });

  it('gives ties to the earlier pane', () => {
    expect(allocate(250, [0.5, 0.5], [180, 180], [50, 50])).toEqual([180, 70]);
  });

  it('shares the floors when even they do not fit', () => {
    expect(allocate(80, [0.6, 0.4], [180, 160], [50, 50])).toEqual([40, 40]);
  });

  it('returns nothing for no children', () => {
    expect(allocate(100, [], [])).toEqual([]);
  });
});

describe('layoutPanes', () => {
  it('stacks the default map over affects with a 1 px handle between', () => {
    // The approved boards at 1280 by 800: the Map pane from y 32 to
    // 380, the line, then the Affects pane down to the vitals at 696.
    const { leaves, handles } = layoutPanes(defaultLayout().root, 300, 664);
    expect(leaves.map((l) => l.leaf.pane)).toEqual(['map', 'affects']);
    expect(leaves[0].rect).toEqual({ x: 0, y: 0, w: 300, h: 348 });
    expect(handles).toHaveLength(1);
    expect(handles[0]).toMatchObject({
      parentId: 'root',
      index: 0,
      dir: 'column',
      rect: { x: 0, y: 348, w: 300, h: 1 },
      sizes: [348, 315],
    });
    expect(leaves[1].rect).toEqual({ x: 0, y: 349, w: 300, h: 315 });
  });

  it('sets a split right pane beside its sibling inside the stack', () => {
    const tree = splitPane(defaultLayout().root, 'affects', 'row', paneRef('group'));
    const { leaves, handles } = layoutPanes(tree, 440, 664);
    const byPane = Object.fromEntries(leaves.map((l) => [l.leaf.pane, l.rect]));
    expect(byPane.map).toEqual({ x: 0, y: 0, w: 440, h: 348 });
    expect(byPane.affects).toEqual({ x: 0, y: 349, w: 220, h: 315 });
    expect(byPane.group).toEqual({ x: 221, y: 349, w: 219, h: 315 });
    const vertical = handles.find((h) => h.dir === 'row');
    expect(vertical?.rect).toEqual({ x: 220, y: 349, w: 1, h: 315 });
  });

  it('keeps a light pane at its minimum on a tall panel', () => {
    const root = { ...defaultLayout().root };
    root.children = [
      { ...root.children[0], weight: 0.95 },
      { ...root.children[1], weight: 0.05 },
    ];
    const { leaves, handles } = layoutPanes(root, 300, 664);
    expect(leaves[1].rect.h).toBe(PANE_MIN_H.affects);
    expect(leaves[0].rect.h).toBe(664 - 1 - PANE_MIN_H.affects);
    expect(handles[0].mins).toEqual([180, 160]);
  });

  it('stacks without overlap when the panel is too short for every minimum', () => {
    const tree = addPane(addPane(defaultLayout().root, paneRef('group')), paneRef('chat'));
    const { leaves } = layoutPanes(tree, 300, 400);
    const rects = leaves.map((l) => l.rect);
    for (let i = 1; i < rects.length; i += 1) {
      expect(rects[i].y).toBe(rects[i - 1].y + rects[i - 1].h + 1);
    }
    const last = rects[rects.length - 1];
    expect(last.y + last.h).toBe(400);
    // Every pane keeps its header and a row.
    for (const r of rects) expect(r.h).toBeGreaterThanOrEqual(PANE_FLOOR_H);
    // The map, heaviest, reads in full.
    expect(leaves[0].leaf.pane).toBe('map');
    expect(rects[0].h).toBeGreaterThanOrEqual(PANE_MIN_H.map);
  });

  it('keeps every harmful affect and every group member in view after adding panes', () => {
    // Add Group, then Chat, at 1280 by 800, with the slots in a fight
    // and four group members.
    const tree = addPane(addPane(defaultLayout().root, paneRef('group')), paneRef('chat'));
    const mins = { affects: affectsMinH(FIGHT_AFFECTS), group: groupMinH(4) };
    const { leaves } = layoutPanes(tree, 300, 664, mins);
    const h = Object.fromEntries(leaves.map((l) => [l.leaf.pane, l.rect.h]));
    expect(h.affects).toBeGreaterThanOrEqual(191);
    expect(h.group).toBeGreaterThanOrEqual(116);
    expect(h.map).toBeGreaterThanOrEqual(PANE_MIN_H.map);
    expect(h.chat).toBeGreaterThanOrEqual(PANE_MIN_H.chat);
    expect(total(Object.values(h)) + 3).toBe(664);
  });

  it('holds the raised minimums with even weights too', () => {
    const tree = sanitize({
      id: 'root',
      split: 'column',
      children: [{ pane: 'map' }, { pane: 'group' }, { pane: 'chat' }, { pane: 'affects' }],
    });
    const mins = { affects: affectsMinH(FIGHT_AFFECTS), group: groupMinH(4) };
    const { leaves, handles } = layoutPanes(tree, 300, 664, mins);
    const h = Object.fromEntries(leaves.map((l) => [l.leaf.pane, l.rect.h]));
    expect(h.affects).toBe(191);
    expect(h.group).toBeGreaterThanOrEqual(116);
    // A drag stops at the raised minimum as well.
    expect(handles[2].mins).toEqual([180, 116, 120, 191]);
  });

  it('lays out nothing for an empty root', () => {
    const empty: PaneSplit = { id: 'root', split: 'column', weight: 1, children: [] };
    expect(layoutPanes(empty, 300, 600)).toEqual({ leaves: [], handles: [] });
  });
});

describe('dragSizes', () => {
  it('trades space between the two neighbours only', () => {
    expect(dragSizes([100, 200, 300], 1, 50, 50)).toEqual([100, 250, 250]);
  });

  it('stops each neighbour at the minimum', () => {
    expect(dragSizes([100, 200], 0, -90, 50)).toEqual([50, 250]);
    expect(dragSizes([100, 200], 0, 900, 50)).toEqual([250, 50]);
  });

  it('stops each neighbour at its own minimum', () => {
    // Map above affects: the map keeps 180, affects keeps 160.
    expect(dragSizes([300, 200], 0, -500, 180, 160)).toEqual([180, 320]);
    expect(dragSizes([300, 200], 0, 500, 180, 160)).toEqual([340, 160]);
  });

  it('leaves a pair with no room to give alone', () => {
    expect(dragSizes([40, 40], 0, 10, 50)).toEqual([40, 40]);
    // A squeezed pair on a short panel does not move.
    expect(dragSizes([180, 119], 0, -20, 180, 160)).toEqual([180, 119]);
  });
});

describe('at your panel size', () => {
  it('keeps every stock minimum and the floor at 12 px', () => {
    for (const pane of PANE_TYPES) expect(paneMinH(pane, 12), pane).toBe(PANE_MIN_H[pane]);
    for (const pane of PANE_TYPES) expect(paneMinH(pane), pane).toBe(PANE_MIN_H[pane]);
    expect(paneMinH('lua', 12)).toBe(PANE_MIN_H.lua);
    expect(paneFloorH(12)).toBe(PANE_FLOOR_H);
    expect(paneFloorH()).toBe(PANE_FLOOR_H);
  });

  it('holds every pane at a 37 px header and its rows at 16 px', () => {
    // Six 29 px rows of affects, three of group and staff queues, and a
    // map and chat body four thirds of 152 and 92 px.
    expect(paneMinH('affects', 16)).toBe(37 + 6 * 29);
    expect(paneMinH('group', 16)).toBe(37 + 3 * 29);
    expect(paneMinH('imm', 16)).toBe(37 + 3 * 29);
    expect(paneMinH('lua', 16)).toBe(37 + 3 * 29);
    expect(paneMinH('map', 16)).toBe(37 + 203);
    expect(paneMinH('chat', 16)).toBe(37 + 123);
    expect(paneFloorH(16)).toBe(37 + 29);
  });

  it('counts the same rows at 12 px as before', () => {
    expect(affectsMinH(FIGHT_AFFECTS, 2, 12)).toBe(28 + 7 * 22 + 9);
    expect(countdownMinH(rows(thirty), 1, 12)).toBe(28 + 6 * 23);
    expect(chipsMinH(rows(thirty), 247, FIXED_MEASURE, 12)).toBe(204);
    expect(groupMinH(12, 12)).toBe(28 + 6 * 22 + 11);
  });

  it('counts taller rows at 16 px', () => {
    // Four rows of slots, the 11 px rule, and three rows for the four
    // harmful affects and the count, each 29 px.
    expect(affectsMinH(FIGHT_AFFECTS, 2, 16)).toBe(37 + 7 * 29 + 11);
    // Down to poison and the count: six rows of 31 px.
    expect(countdownMinH(rows(thirty), 1, 16)).toBe(37 + 6 * 31);
    const body = chipsMinBody(
      chipGroups(rows(thirty)),
      247,
      (r) => affectHours(r.state, r.ticks),
      FIXED_MEASURE,
      'runin',
      12 * 29,
      16,
    );
    expect(chipsMinH(rows(thirty), 247, FIXED_MEASURE, 16)).toBe(37 + body);
    expect(37 + body).toBeGreaterThan(204);
    // Six members of 29 px, and half a row of the seventh.
    expect(groupMinH(4, 16)).toBe(37 + 4 * 29);
    expect(groupMinH(12, 16)).toBe(37 + 6 * 29 + 15);
  });

  it('lays every pane out at its minimum at 16 px, and at its floor on a short panel', () => {
    const tree = addPane(addPane(defaultLayout().root, paneRef('group')), paneRef('chat'));
    const tall = layoutPanes(tree, 300, 900, {}, 16);
    for (const { leaf, rect } of tall.leaves) {
      expect(rect.h, leaf.pane).toBeGreaterThanOrEqual(paneMinH(leaf.pane, 16));
    }
    expect(tall.handles[0].mins).toEqual([240, 211, 124, 160]);
    expect(fitsPanel(tree, 300, 900, 16)).toBe(true);
    expect(fitsPanel(tree, 300, 700, 16)).toBe(false);
    const short = layoutPanes(tree, 300, 400, {}, 16);
    const rects = short.leaves.map((l) => l.rect);
    for (let i = 1; i < rects.length; i += 1) {
      expect(rects[i].y).toBe(rects[i - 1].y + rects[i - 1].h + 1);
    }
    for (const r of rects) expect(r.h).toBeGreaterThanOrEqual(paneFloorH(16));
  });

  it('counts one column below the wider pane two columns need', () => {
    // A 400 px pane draws two columns at 12 px and one at 16 px.
    expect(affectsStyleMinH(FIGHT_AFFECTS, 400, 'timers')).toBe(affectsMinH(FIGHT_AFFECTS, 2));
    expect(affectsStyleMinH(FIGHT_AFFECTS, 400, 'timers', FIXED_MEASURE, 16)).toBe(
      affectsMinH(FIGHT_AFFECTS, 1, 16),
    );
    const tree = splitPane(defaultLayout().root, 'affects', 'row', paneRef('group'));
    expect(affectsMinIn(tree, 494, rows(thirty), 'countdown', FIXED_MEASURE, 16)).toBe(
      countdownMinH(rows(thirty), 1, 16),
    );
  });
});
