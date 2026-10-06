import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import fixture from '../../fixtures/pane-layout/sanitize.json';
import {
  addPane,
  allPanes,
  closePane,
  countPanes,
  defaultLayout,
  findNode,
  isLeaf,
  leafIdFor,
  leafKey,
  paneKey,
  paneRef,
  replacePane,
  sanitize,
  sanitizeLayout,
  setWeights,
  splitPane,
  type PaneLayout,
  type PaneLeaf,
  type PaneNode,
  type PaneRef,
  type PaneSplit,
} from './paneLayout';

const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: tauri.invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
    tauri.handlers.set(name, handler);
    return Promise.resolve(() => tauri.handlers.delete(name));
  }),
}));

// The shape serde writes: empty props, empty children and a null
// width are left out.
function wire(layout: PaneLayout): unknown {
  const node = (n: PaneNode): unknown =>
    isLeaf(n)
      ? {
          id: n.id,
          pane: n.pane,
          weight: n.weight,
          ...(Object.keys(n.props).length > 0 ? { props: n.props } : {}),
        }
      : {
          id: n.id,
          split: n.split,
          weight: n.weight,
          ...(n.children.length > 0 ? { children: n.children.map(node) } : {}),
        };
  return {
    version: layout.version,
    panel_open: layout.panel_open,
    ...(layout.panel_width !== null ? { panel_width: layout.panel_width } : {}),
    root: node(layout.root),
  };
}

// Compact shape for asserting a tree: pane type and weight per leaf,
// split direction per split.
function shape(node: PaneNode): unknown {
  return isLeaf(node)
    ? [node.pane, node.weight]
    : { [node.split]: node.children.map((c) => shape(c)) };
}

function deepFreeze<T>(value: T): T {
  if (value !== null && typeof value === 'object') {
    Object.values(value).forEach(deepFreeze);
    Object.freeze(value);
  }
  return value;
}

const root = (): PaneSplit => deepFreeze(defaultLayout().root);

describe('sanitize', () => {
  // The same cases run against PaneLayoutPersist::sanitize in Rust.
  for (const c of fixture.cases) {
    it(c.name, () => {
      const once = sanitizeLayout(c.input);
      expect(wire(once)).toEqual(c.expected);
      expect(sanitizeLayout(once)).toEqual(once);
    });
  }

  it('accepts anything and always returns a split root', () => {
    for (const junk of [null, 42, 'map', [], { pane: 7 }, { children: 'no' }]) {
      const tree = sanitize(junk);
      expect(isLeaf(tree)).toBe(false);
      expect(tree.children).toEqual([]);
    }
  });

  it('sorts props by key so a round trip through Rust compares equal', () => {
    const tree = sanitize({ pane: 'chat', props: { b: '2', a: '1', skip: 3 } });
    const chat = tree.children[0];
    expect(isLeaf(chat) && Object.keys(chat.props)).toEqual(['a', 'b']);
  });
});

describe('splitPane', () => {
  it('splits right into a row that halves the old share', () => {
    const next = splitPane(root(), 'affects', 'row', paneRef('group'));
    expect(shape(next)).toEqual({
      column: [
        ['map', 0.525],
        {
          row: [
            ['affects', 0.5],
            ['group', 0.5],
          ],
        },
      ],
    });
    expect(findNode(next, 'group')).not.toBeNull();
    expect(findNode(next, 'split')).not.toBeNull();
  });

  it('adds a sibling when the parent already runs that way', () => {
    const next = splitPane(root(), 'affects', 'column', paneRef('chat'));
    expect(shape(next)).toEqual({
      column: [
        ['map', 0.525],
        ['affects', 0.2375],
        ['chat', 0.2375],
      ],
    });
  });

  it('moves a pane already shown elsewhere', () => {
    const three = addPane(root(), paneRef('group'));
    const next = splitPane(three, 'affects', 'row', paneRef('group'));
    expect(allPanes(next)).toEqual(['map', 'affects', 'group']);
    expect(shape(next)).toEqual({
      column: [
        ['map', 0.525],
        {
          row: [
            ['affects', 0.5],
            ['group', 0.5],
          ],
        },
      ],
    });
  });

  it('leaves the tree alone for the root, an unknown id, or the same pane', () => {
    const tree = root();
    expect(splitPane(tree, 'root', 'row', paneRef('group'))).toBe(tree);
    expect(splitPane(tree, 'nope', 'row', paneRef('group'))).toBe(tree);
    expect(splitPane(tree, 'map', 'row', paneRef('map'))).toBe(tree);
  });
});

describe('closePane', () => {
  it('gives the space to the siblings', () => {
    expect(shape(closePane(root(), 'affects'))).toEqual({ column: [['map', 1]] });
  });

  it('collapses a split left with one child', () => {
    const split = splitPane(root(), 'affects', 'row', paneRef('group'));
    const next = closePane(split, 'group');
    expect(next).toEqual(root());
  });

  it('empties the panel when the last pane or the root closes', () => {
    const one = closePane(root(), 'affects');
    const none = closePane(one, 'map');
    expect(none).toEqual({ id: 'root', split: 'column', weight: 1, children: [] });
    expect(closePane(root(), 'root').children).toEqual([]);
  });

  it('leaves the tree alone for an unknown id', () => {
    const tree = root();
    expect(closePane(tree, 'nope')).toBe(tree);
  });
});

describe('replacePane', () => {
  it('keeps the id and share and resets props', () => {
    const withProps = sanitize({
      id: 'root',
      split: 'column',
      children: [
        { id: 'map', pane: 'map', weight: 0.6 },
        { id: 'talk', pane: 'chat', weight: 0.4, props: { channel: 'tell' } },
      ],
    });
    const next = replacePane(withProps, 'talk', paneRef('group'));
    expect(findNode(next, 'talk')).toEqual({ id: 'talk', pane: 'group', weight: 0.4, props: {} });
  });

  it('moves a pane already shown elsewhere', () => {
    const next = replacePane(root(), 'affects', paneRef('map'));
    expect(allPanes(next)).toEqual(['map']);
    expect(next.children[0].id).toBe('affects');
  });

  it('leaves the tree alone for a split, an unknown id, or the same pane', () => {
    const tree = root();
    expect(replacePane(tree, 'root', paneRef('chat'))).toBe(tree);
    expect(replacePane(tree, 'nope', paneRef('chat'))).toBe(tree);
    expect(replacePane(tree, 'map', paneRef('map'))).toBe(tree);
  });
});

describe('setWeights', () => {
  it('normalizes pixel sizes into shares', () => {
    expect(shape(setWeights(root(), 'root', [300, 100]))).toEqual({
      column: [
        ['map', 0.75],
        ['affects', 0.25],
      ],
    });
  });

  it('ignores a length mismatch, a bad weight, or a leaf', () => {
    const tree = root();
    expect(setWeights(tree, 'root', [1])).toBe(tree);
    expect(setWeights(tree, 'root', [1, 0])).toBe(tree);
    expect(setWeights(tree, 'root', [1, Number.NaN])).toBe(tree);
    expect(setWeights(tree, 'map', [])).toBe(tree);
  });
});

describe('addPane', () => {
  it('appends to the root column with a share sized to its reading height', () => {
    // Group reads at 94 px against 340 for the map and affects above it.
    expect(shape(addPane(root(), paneRef('group')))).toEqual({
      column: [
        ['map', 0.4113],
        ['affects', 0.3721],
        ['group', 0.2166],
      ],
    });
  });

  it('keeps the panes above in step as more panes join', () => {
    const next = addPane(addPane(root(), paneRef('group')), paneRef('chat'));
    expect(shape(next)).toEqual({
      column: [
        ['map', 0.3222],
        ['affects', 0.2915],
        ['group', 0.1697],
        ['chat', 0.2166],
      ],
    });
  });

  it('fills an empty panel', () => {
    const empty = closePane(root(), 'root');
    expect(shape(addPane(empty, paneRef('chat')))).toEqual({ column: [['chat', 1]] });
  });

  it('nests a row root under a new column and keeps the root id', () => {
    const row = sanitize({
      id: 'root',
      split: 'row',
      children: [{ pane: 'map' }, { pane: 'group' }],
    });
    const next = addPane(row, paneRef('affects'));
    expect(next.id).toBe('root');
    // The row reads at its tallest pane, the map's 180 px.
    expect(shape(next)).toEqual({
      column: [
        {
          row: [
            ['map', 0.5],
            ['group', 0.5],
          ],
        },
        ['affects', 0.4706],
      ],
    });
  });

  it('leaves the tree alone when the pane is already shown', () => {
    const tree = root();
    expect(addPane(tree, paneRef('map'))).toBe(tree);
  });
});

describe('tree operations', () => {
  it('never mutate their input and keep untouched ids', () => {
    // root() is deep frozen, so any write inside an operation throws.
    let tree = root();
    tree = deepFreeze(splitPane(tree, 'affects', 'row', paneRef('group')));
    tree = deepFreeze(addPane(tree, paneRef('chat')));
    tree = deepFreeze(setWeights(tree, 'root', [2, 1, 1]));
    tree = deepFreeze(replacePane(tree, 'chat', paneRef('imm')));
    tree = deepFreeze(closePane(tree, 'group'));
    expect(findNode(tree, 'map')).not.toBeNull();
    expect(findNode(tree, 'affects')).not.toBeNull();
    expect(allPanes(tree)).toEqual(['map', 'affects', 'imm']);
  });
});

describe('Lua panes', () => {
  const lua = (id: string, title: string): PaneRef => ({
    pane: 'lua',
    props: { id, plugin: 'weather_pane', title },
  });
  const weather = lua('weather', 'Weather');
  const tides = lua('tides', 'Tides');
  const luaLeaves = (tree: PaneSplit) =>
    allPanes(tree).filter((key) => key.startsWith('lua:')).length;

  it('key on their plugin and id, never on their title', () => {
    expect(paneKey(weather)).toBe(paneKey(lua('weather', 'Rain')));
    expect(paneKey(weather)).not.toBe(paneKey(tides));
    expect(paneKey({ pane: 'lua', props: { plugin: 'other', id: 'weather' } })).not.toBe(
      paneKey(weather),
    );
    expect(paneKey(paneRef('chat'))).toBe('chat');
  });

  it('add once per key, with the props of the reference', () => {
    const one = addPane(root(), weather);
    const two = addPane(one, tides);
    expect(addPane(two, lua('weather', 'Rain'))).toBe(two);
    expect(luaLeaves(two)).toBe(2);
    expect(leafIdFor(two, weather)).toBe('lua');
    expect(leafIdFor(two, tides)).toBe('lua-2');
    expect(findNode(two, 'lua')).toMatchObject({ pane: 'lua', props: weather.props });
  });

  it('split in a second Lua pane and move one already shown', () => {
    const tree = addPane(root(), weather);
    const split = splitPane(tree, 'lua', 'row', tides);
    expect(luaLeaves(split)).toBe(2);
    const moved = splitPane(split, 'map', 'row', weather);
    expect(luaLeaves(moved)).toBe(2);
    expect(shape(moved)).toEqual({
      column: [
        {
          row: [
            ['map', 0.5],
            ['lua', 0.5],
          ],
        },
        expect.anything(),
        expect.anything(),
      ],
    });
    expect(splitPane(moved, leafIdFor(moved, weather) ?? '', 'row', weather)).toBe(moved);
  });

  it('move to the leaf you show here instead', () => {
    const tree = addPane(addPane(root(), weather), tides);
    const next = replacePane(tree, 'affects', weather);
    expect(luaLeaves(next)).toBe(2);
    expect(leafIdFor(next, weather)).toBe('affects');
    expect(findNode(next, 'affects')).toMatchObject({ pane: 'lua', props: weather.props });
    expect(findNode(next, 'lua')).toBeNull();
    expect(replacePane(next, 'affects', weather)).toBe(next);
  });

  it('leave a tree of built-in panes byte for byte as before', () => {
    let tree = root();
    tree = splitPane(tree, 'affects', 'row', paneRef('group'));
    tree = addPane(tree, paneRef('chat'));
    tree = splitPane(tree, 'chat', 'row', paneRef('imm'));
    tree = replacePane(tree, 'map', paneRef('group'));
    tree = setWeights(tree, 'root', [3, 2, 1]);
    tree = closePane(tree, 'imm');
    tree = addPane(tree, paneRef('map'));
    // What these calls gave when they took a bare pane type.
    expect(JSON.stringify(tree)).toBe(
      '{"id":"root","split":"column","weight":1,"children":[' +
        '{"id":"map","pane":"group","weight":0.3375,"props":{}},' +
        '{"id":"affects","pane":"affects","weight":0.225,"props":{}},' +
        '{"id":"chat","pane":"chat","weight":0.1125,"props":{}},' +
        '{"id":"map-2","pane":"map","weight":0.3249,"props":{}}]}',
    );
  });
});

describe('Chat panes', () => {
  const chat = paneRef('chat');
  const withChat = () => deepFreeze(addPane(root(), chat));

  it('add another under a Chat, up to four', () => {
    const two = addPane(withChat(), chat);
    expect(allPanes(two)).toEqual(['map', 'affects', 'chat', 'chat']);
    expect(findNode(two, 'chat-2')).toMatchObject({ pane: 'chat', props: {} });
    const four = addPane(addPane(two, chat), chat);
    expect(countPanes(four, chat)).toBe(4);
    expect(addPane(four, chat)).toBe(four);
  });

  it('split in beside a Chat and leave the first in place', () => {
    const tree = withChat();
    const next = splitPane(tree, 'affects', 'row', chat);
    expect(findNode(next, 'chat')).toEqual(findNode(tree, 'chat'));
    expect(shape(next)).toEqual({
      column: [
        ['map', expect.any(Number)],
        {
          row: [
            ['affects', 0.5],
            ['chat', 0.5],
          ],
        },
        ['chat', expect.any(Number)],
      ],
    });
    expect(leafIdFor(next, chat)).toBe('chat-2');
    expect(splitPane(next, 'chat', 'column', chat)).not.toBe(next);
  });

  it('show here instead of Map and keep the other Chat', () => {
    const tree = withChat();
    const next = replacePane(tree, 'map', chat);
    expect(allPanes(next)).toEqual(['chat', 'affects', 'chat']);
    expect(findNode(next, 'chat')).toEqual(findNode(tree, 'chat'));
    expect(replacePane(next, 'chat', chat)).toBe(next);
  });

  it('stop at four from a split or show here instead', () => {
    let four = withChat();
    for (let i = 0; i < 3; i += 1) four = addPane(four, chat);
    expect(splitPane(four, 'map', 'row', chat)).toBe(four);
    expect(replacePane(four, 'map', chat)).toBe(four);
  });

  it('key each box on its leaf, and every other pane on its type', () => {
    const two = addPane(withChat(), chat);
    expect(leafKey(findNode(two, 'chat') as PaneLeaf)).toBe('chat#chat');
    expect(leafKey(findNode(two, 'chat-2') as PaneLeaf)).toBe('chat#chat-2');
    expect(leafKey(findNode(two, 'map') as PaneLeaf)).toBe('map');
  });
});

describe('persistence', () => {
  const layoutWith = (...panes: string[]): PaneLayout =>
    sanitizeLayout({
      root: { id: 'root', split: 'column', children: panes.map((pane) => ({ pane })) },
    });

  const flushMicrotasks = async () => {
    for (let i = 0; i < 20; i += 1) await Promise.resolve();
  };

  const emit = (name: string, payload: unknown) => tauri.handlers.get(name)?.({ payload });

  // Fresh module state per test, since the sync state is module level.
  const load = async () => {
    vi.resetModules();
    return import('./paneLayout');
  };

  beforeEach(() => {
    vi.useFakeTimers();
    tauri.invoke.mockReset();
    tauri.handlers.clear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('coalesces a drag into one write after 250 ms', async () => {
    const mod = await load();
    tauri.invoke.mockResolvedValue(undefined);
    mod.setPaneLayout(layoutWith('map'));
    mod.setPaneLayout(layoutWith('map', 'chat'));
    vi.advanceTimersByTime(249);
    expect(tauri.invoke).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(tauri.invoke).toHaveBeenCalledTimes(1);
    expect(tauri.invoke).toHaveBeenCalledWith('pane_layout_set', {
      layout: layoutWith('map', 'chat'),
      generation: null,
    });
  });

  it('keeps a valid generation off the wire and drops a bad one', () => {
    expect(sanitizeLayout({ generation: 7 }).generation).toBe(7);
    expect('generation' in sanitizeLayout({ generation: -1 })).toBe(false);
    expect('generation' in sanitizeLayout({ generation: '7' })).toBe(false);
  });

  it('sends the generation the edited tree came with', async () => {
    const mod = await load();
    tauri.invoke.mockResolvedValue(true);
    mod.setPaneLayout({ ...layoutWith('map'), generation: 4 });
    vi.advanceTimersByTime(250);
    expect(tauri.invoke).toHaveBeenCalledWith('pane_layout_set', {
      layout: { ...layoutWith('map'), generation: 4 },
      generation: 4,
    });
  });

  it('reloads the tree when the backend refuses a write from a swapped profile', async () => {
    const mod = await load();
    const seen: PaneLayout[] = [];
    await mod.subscribePaneLayout((l) => seen.push(l));
    const current = { ...layoutWith('group', 'chat'), generation: 5 };
    tauri.invoke.mockImplementation((cmd: string) =>
      Promise.resolve(cmd === 'pane_layout_set' ? false : current),
    );
    mod.setPaneLayout({ ...layoutWith('map'), generation: 4 });
    vi.advanceTimersByTime(250);
    await flushMicrotasks();
    expect(tauri.invoke).toHaveBeenCalledWith('pane_layout_get');
    expect(seen).toEqual([current]);
  });

  it('holds back an echo while a write is out and skips it once it matches', async () => {
    const mod = await load();
    const seen: PaneLayout[] = [];
    await mod.subscribePaneLayout((l) => seen.push(l));
    let finishWrite = () => {};
    tauri.invoke.mockImplementation((cmd: string) =>
      cmd === 'pane_layout_set'
        ? new Promise<void>((resolve) => {
            finishWrite = () => resolve();
          })
        : Promise.resolve(layoutWith('map', 'chat')),
    );
    mod.setPaneLayout(layoutWith('map', 'chat'));
    vi.advanceTimersByTime(250);
    emit('vosh://pane-layout-changed', layoutWith('map', 'chat'));
    expect(seen).toEqual([]);
    finishWrite();
    await flushMicrotasks();
    // The settle fetch returned what this window wrote, so nothing new.
    expect(tauri.invoke).toHaveBeenCalledWith('pane_layout_get');
    expect(seen).toEqual([]);
  });

  it('delivers a change from elsewhere when nothing is pending', async () => {
    const mod = await load();
    const seen: PaneLayout[] = [];
    await mod.subscribePaneLayout((l) => seen.push(l));
    emit('vosh://pane-layout-changed', layoutWith('group'));
    expect(seen).toEqual([layoutWith('group')]);
  });

  it('drops a pending write on a profile switch and loads the new profile', async () => {
    const mod = await load();
    const seen: PaneLayout[] = [];
    await mod.subscribePaneLayout((l) => seen.push(l));
    tauri.invoke.mockResolvedValue(layoutWith('group', 'chat'));
    mod.setPaneLayout(layoutWith('map'));
    // The backend sends the new profile's tree, then the switch.
    emit('vosh://pane-layout-changed', layoutWith('group', 'chat'));
    expect(seen).toEqual([]);
    emit('vosh://profile-switched', 'alt');
    await flushMicrotasks();
    expect(seen).toEqual([layoutWith('group', 'chat')]);
    vi.advanceTimersByTime(1000);
    expect(tauri.invoke).not.toHaveBeenCalledWith('pane_layout_set', expect.anything());
  });

  it('resets the panes of the live profile', async () => {
    const mod = await load();
    tauri.invoke.mockResolvedValue({ ...defaultLayout(), generation: 4 });
    expect(await mod.resetPaneLayout()).toEqual({ ...defaultLayout(), generation: 4 });
    expect(tauri.invoke).toHaveBeenLastCalledWith('pane_layout_reset', { profile: null });
  });
});
