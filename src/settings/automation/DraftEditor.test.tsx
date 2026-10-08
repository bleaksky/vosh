import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { act, createElement } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { foldedStorageKey, groupKeyOf, searchText } from '../../automation/automationList';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';
import type { DetailProps, KindSpec } from './types';

// The Automation list folds its groups through the editor that owns
// the filter and the selection. This mounts the editor on a list of
// things, the way a kind's page does, and drives it through the
// handlers React keeps on each element, since this DOM sends no events.

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  emit: vi.fn(() => Promise.resolve()),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));

interface Thing {
  name: string;
  group: string;
}

const THINGS: Thing[] = [
  { name: 'Echo my deaths', group: '' },
  { name: 'Flee below 20 percent', group: 'combat' },
  { name: 'Loot every kill', group: 'combat' },
  { name: 'Sleep when mana is low', group: 'idle' },
];

/** The things the next mount loads. */
let things: Thing[] = THINGS;

/** What the editor last passed to onError, in order. */
const errors: (string | null)[] = [];

/** The detail card the editor shows last, to change its item. */
let detail: DetailProps<Thing> | null = null;

const SPEC: KindSpec<Thing> = {
  id: 'things',
  noun: { one: 'thing', many: 'things' },
  filterLabel: 'Filter things',
  newLabel: 'New thing',
  emptyDetail: 'Choose a thing to edit it.',
  emptyList: 'You have no things yet.',
  load: () => Promise.resolve(things),
  save: () => Promise.resolve(),
  entry: (t) => ({
    name: t.name,
    group: groupKeyOf(t.group),
    enabled: true,
    text: searchText(t.name, t.group),
  }),
  keyOf: (t) => t.name,
  blank: () => ({ name: '', group: '' }),
  renderDetail: (props) => {
    detail = props;
    return null;
  },
};

const doc = new FakeDocument();
const store = new Map<string, string>();
let createRoot: typeof import('react-dom/client').createRoot;
let DraftEditor: typeof import('./DraftEditor').DraftEditor;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
    setTimeout: globalThis.setTimeout.bind(globalThis),
    clearTimeout: globalThis.clearTimeout.bind(globalThis),
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, String(value)),
    removeItem: (key: string) => void store.delete(key),
  });
  vi.stubGlobal('CSS', { escape: (text: string) => text });
  // The list finds a row or a heading by one data attribute.
  (FakeElement.prototype as unknown as { querySelector: unknown }).querySelector = function (
    this: FakeElement,
    selector: string,
  ) {
    const m = /^\[(data-[a-z]+)="(.*)"\]$/.exec(selector);
    if (!m) return null;
    return findAll(this, (el) => el.getAttribute(m[1]) === m[2])[0] ?? null;
  };
  ({ createRoot } = await import('react-dom/client'));
  ({ DraftEditor } = await import('./DraftEditor'));
});

type Handler = (e?: unknown) => void;

/** The handlers React keeps on an element. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
  store.clear();
  things = THINGS;
  detail = null;
  errors.length = 0;
});

async function mount(spec: KindSpec<Thing> = SPEC) {
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    root.render(
      createElement(DraftEditor<Thing>, {
        spec,
        json: false,
        onJson: () => {},
        onDirty: () => {},
        onError: (message: string | null) => void errors.push(message),
      }),
    );
  });
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  // A browser fires focus as an element takes it. This DOM does not, so
  // hand the element that took focus its React focus handler.
  const run = (fn: () => void) =>
    act(async () => {
      const before = doc.activeElement;
      fn();
      const now = doc.activeElement;
      if (now && now !== before) on(now).onFocus?.({ target: now });
    });
  const heading = (key: string) =>
    findAll(container, (el) => el.getAttribute('data-fold') === key)[0] ?? null;
  const row = (name: string) =>
    findAll(container, (el) => el.hasAttribute('data-uid') && el.textContent.startsWith(name))[0] ??
    null;
  const filter = findAll(container, (el) => el.nodeName === 'INPUT')[0];
  const scroll = findAll(container, (el) => el.getAttribute('class') === 'st-auto-scroll')[0];
  const groupSwitch = (group: string) =>
    findAll(container, (el) => el.getAttribute('data-group-switch') === group)[0] ?? null;
  return {
    /** The button that reads `label`, such as Save. */
    button: (label: string) =>
      findAll(container, (el) => el.nodeName === 'BUTTON' && el.textContent === label)[0] ?? null,
    heading,
    row,
    groupSwitch,
    /** The note under a heading whose group the loadouts decide. */
    notes: () =>
      findAll(container, (el) => el.getAttribute('class') === 'st-auto-groupnote').map(
        (el) => el.textContent,
      ),
    /** Flip a group switch, as a click does. */
    flip: (group: string) =>
      run(() => {
        const el = groupSwitch(group);
        if (!el) throw new Error(`no switch for ${group}`);
        const props = on(el) as unknown as { checked: boolean; onChange: Handler };
        props.onChange({ target: { checked: !props.checked } });
      }),
    /** Whether a group heading shows its group open. */
    open: (key: string) => heading(key)?.getAttribute('aria-expanded') === 'true',
    /** The names of the rows that show. */
    rows: () =>
      findAll(container, (el) => el.hasAttribute('data-uid')).map((el) =>
        el.textContent.replace(/(On|Off)$/, ''),
      ),
    selected: () =>
      findAll(container, (el) => el.getAttribute('aria-current') === 'true')[0]?.textContent ?? '',
    click: (el: FakeElement | null) =>
      run(() => {
        if (!el) throw new Error('nothing to click');
        on(el).onClick({ currentTarget: el, preventDefault() {} });
      }),
    type: (text: string) => run(() => on(filter).onChange({ target: { value: text } })),
    /** Press a key with focus on a heading or a row. */
    key: (key: string, at: FakeElement | null) =>
      run(() => {
        if (!at) throw new Error('nothing has focus');
        const dataset: Record<string, string> = {};
        const uid = at.getAttribute('data-uid');
        const fold = at.getAttribute('data-fold');
        const groupSwitch = at.getAttribute('data-group-switch');
        if (uid !== null) dataset.uid = uid;
        if (fold !== null) dataset.fold = fold;
        if (groupSwitch !== null) dataset.groupSwitch = groupSwitch;
        on(scroll).onKeyDown({ key, target: { dataset }, preventDefault() {} });
      }),
  };
}

const KEY = foldedStorageKey('things');

describe('folding a group in the Automation list', () => {
  it('folds a group from its heading and remembers it for the list', async () => {
    const list = await mount();
    expect(list.open('g:combat')).toBe(true);
    await list.click(list.heading('g:combat'));
    expect(list.open('g:combat')).toBe(false);
    expect(list.rows()).toEqual(['Echo my deaths', 'Sleep when mana is low']);
    expect(store.get(KEY)).toBe('["g:combat"]');

    // The list opens again with the group still folded.
    const again = await mount();
    expect(again.open('g:combat')).toBe(false);
    await again.click(again.heading('g:combat'));
    expect(again.open('g:combat')).toBe(true);
    expect(store.has(KEY)).toBe(false);
  });

  it('folds with Left, opens with Right, and steps through headings and rows', async () => {
    const list = await mount();
    await list.key('ArrowLeft', list.heading('g:idle'));
    expect(list.open('g:idle')).toBe(false);
    await list.key('ArrowRight', list.heading('g:idle'));
    expect(list.open('g:idle')).toBe(true);

    // Down from the first row lands on the combat heading and leaves the
    // selection where it was. Down again selects the first combat row.
    expect(list.selected()).toMatch(/^Echo my deaths/);
    await list.key('ArrowDown', list.row('Echo my deaths'));
    expect(list.heading('g:combat')?.getAttribute('tabindex')).toBe('0');
    expect(list.selected()).toMatch(/^Echo my deaths/);
    await list.key('ArrowDown', list.heading('g:combat'));
    expect(list.selected()).toMatch(/^Flee below 20 percent/);

    // A folded group's rows leave the order, so Down skips them.
    await list.click(list.heading('g:combat'));
    await list.key('ArrowDown', list.heading('g:combat'));
    expect(doc.activeElement?.getAttribute('data-fold')).toBe('g:idle');
    await list.key('ArrowDown', list.heading('g:idle'));
    expect(list.selected()).toMatch(/^Sleep when mana is low/);
  });

  it('opens a folded group with a match while the filter has text, then folds it again', async () => {
    store.set(KEY, '["g:combat","g:idle"]');
    const list = await mount();
    expect(list.rows()).toEqual(['Echo my deaths']);
    await list.type('loot');
    expect(list.open('g:combat')).toBe(true);
    expect(list.rows()).toEqual(['Loot every kill']);
    await list.type('');
    expect(list.open('g:combat')).toBe(false);
    expect(list.open('g:idle')).toBe(false);
    expect(store.get(KEY)).toBe('["g:combat","g:idle"]');
  });

  it('keeps open the group of a row you pick from the matches', async () => {
    store.set(KEY, '["g:combat","g:idle"]');
    const list = await mount();
    await list.type('loot');
    await list.click(list.row('Loot every kill'));
    await list.type('');
    expect(list.open('g:combat')).toBe(true);
    expect(list.open('g:idle')).toBe(false);
    expect(list.selected()).toMatch(/^Loot every kill/);
    expect(store.get(KEY)).toBe('["g:idle"]');
  });

  it('folds a group among the matches only until the filter text changes', async () => {
    const list = await mount();
    await list.type('e');
    await list.click(list.heading('g:combat'));
    expect(list.open('g:combat')).toBe(false);
    expect(store.has(KEY)).toBe(false);
    await list.type('el');
    await list.type('e');
    expect(list.open('g:combat')).toBe(true);
    await list.type('');
    expect(list.open('g:combat')).toBe(true);
  });

  it('opens the folded group an item moves to, so its row shows', async () => {
    store.set(KEY, '["g:idle"]');
    const list = await mount();
    await list.click(list.row('Flee below 20 percent'));
    const card = detail;
    if (!card) throw new Error('no detail card');
    await act(async () => {
      card.update((t) => ({ ...t, group: 'idle' }));
      card.revealInList();
    });
    expect(list.open('g:idle')).toBe(true);
    expect(list.selected()).toMatch(/^Flee below 20 percent/);
    expect(store.has(KEY)).toBe(false);
  });

  it('keeps the selection and the Tab stop when you open a group with every group folded', async () => {
    things = THINGS.filter((t) => t.group !== '');
    store.set(KEY, '["g:combat","g:idle"]');
    const list = await mount();
    expect(list.rows()).toEqual([]);
    // The first row a folded group hides takes the selection, and its
    // heading takes Tab.
    expect(detail?.value.name).toBe('Flee below 20 percent');
    expect(list.heading('g:combat')?.getAttribute('tabindex')).toBe('0');

    await list.key('ArrowDown', list.heading('g:combat'));
    expect(list.heading('g:idle')?.getAttribute('tabindex')).toBe('0');
    await list.key('ArrowRight', list.heading('g:idle'));
    expect(list.rows()).toEqual(['Sleep when mana is low']);
    expect(detail?.value.name).toBe('Flee below 20 percent');
    expect(list.selected()).toBe('');
    expect(list.heading('g:idle')?.getAttribute('tabindex')).toBe('0');
    expect(list.row('Sleep when mana is low')?.getAttribute('tabindex')).toBe('-1');

    // A click opens a group the same way.
    await list.click(list.heading('g:combat'));
    expect(detail?.value.name).toBe('Flee below 20 percent');
    expect(list.selected()).toMatch(/^Flee below 20 percent/);
  });

  it('never folds the ungrouped items at the top', async () => {
    const list = await mount();
    expect(list.row('Echo my deaths')).not.toBeNull();
    expect(list.heading('u:')).toBeNull();
    expect(list.heading('g:combat')).not.toBeNull();
  });
});

describe('the switch on a group heading', () => {
  const GROUPED: KindSpec<Thing> = { ...SPEC, groups: 'triggers' };
  /** The switches the fake store holds, as groups_list answers. */
  let switches: { name: string; enabled: boolean; loadouts?: { on: boolean; by: string[] } }[];
  const sets: unknown[] = [];

  function fakeStore() {
    switches = [
      { name: 'combat', enabled: true },
      { name: 'idle', enabled: true },
    ];
    sets.length = 0;
    vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === 'groups_list') return Promise.resolve(switches);
      if (cmd === 'groups_set_enabled') {
        sets.push(args);
        const { group, enabled } = args as { group: string; enabled: boolean };
        switches = switches.map((s) => (s.name === group ? { ...s, enabled } : s));
        return Promise.resolve(switches);
      }
      return Promise.resolve();
    });
  }

  afterEach(() => {
    vi.mocked(invoke).mockImplementation(() => Promise.resolve());
  });

  const isOn = (el: FakeElement | null) =>
    el ? (on(el) as unknown as { checked: boolean }).checked : null;

  it('turns a whole group at once and leaves the draft as it was', async () => {
    fakeStore();
    const list = await mount(GROUPED);
    expect(isOn(list.groupSwitch('combat'))).toBe(true);
    await list.flip('combat');
    expect(sets).toEqual([{ list: 'triggers', group: 'combat', enabled: false }]);
    expect(isOn(list.groupSwitch('combat'))).toBe(false);
    // The rows stay, and folding works as before.
    expect(list.rows()).toEqual([
      'Echo my deaths',
      'Flee below 20 percent',
      'Loot every kill',
      'Sleep when mana is low',
    ]);
    await list.click(list.heading('g:combat'));
    expect(list.open('g:combat')).toBe(false);
    expect(list.groupSwitch('combat')).not.toBeNull();
  });

  it('follows #group and the loadouts from anywhere', async () => {
    fakeStore();
    const list = await mount(GROUPED);
    switches = [
      { name: 'combat', enabled: false, loadouts: { on: false, by: ['Healer'] } },
      { name: 'idle', enabled: true },
    ];
    const heard = vi
      .mocked(listen)
      .mock.calls.filter(([event]) => event === 'vosh://groups-changed')
      .map(([, cb]) => cb as (e: unknown) => void);
    expect(heard.length).toBeGreaterThan(0);
    await act(async () => heard[heard.length - 1]({ payload: '' }));
    expect(isOn(list.groupSwitch('combat'))).toBe(false);
    expect(on(list.groupSwitch('combat') as FakeElement).disabled).toBeFalsy();
    expect(list.notes()).toEqual(['The Healer loadout leaves this group off.']);
  });

  it.each(['triggers', 'aliases', 'timers'] as const)(
    'turns a %s group the loadouts decide, and says when they turn it back',
    async (groups) => {
      fakeStore();
      switches = [
        { name: 'combat', enabled: true },
        { name: 'idle', enabled: false, loadouts: { on: false, by: ['Healer'] } },
      ];
      const list = await mount({ ...SPEC, groups });
      expect(on(list.groupSwitch('idle') as FakeElement).disabled).toBeFalsy();
      expect(list.notes()).toEqual(['The Healer loadout leaves this group off.']);
      await list.flip('idle');
      expect(sets).toEqual([{ list: groups, group: 'idle', enabled: true }]);
      expect(isOn(list.groupSwitch('idle'))).toBe(true);
      expect(list.notes()).toEqual([
        'The Healer loadout turns this group off again when you next launch Vosh, switch profiles, or save Loadouts.',
      ]);
    },
  );

  it('says when the switches cannot load, and shows none', async () => {
    fakeStore();
    const answer = vi.mocked(invoke).getMockImplementation();
    vi.mocked(invoke).mockImplementation((cmd, args) =>
      cmd === 'groups_list' ? Promise.reject(new Error('no profile')) : answer!(cmd, args),
    );
    const list = await mount(GROUPED);
    // The switches load once as the editor mounts and again once the list
    // loads, and each try says so.
    expect(new Set(errors)).toEqual(
      new Set(["Vosh couldn't load your group switches. Close Settings and open it again."]),
    );
    expect(list.groupSwitch('combat')).toBeNull();
    expect(list.groupSwitch('idle')).toBeNull();
    expect(list.rows()).toHaveLength(4);
  });

  it('moves with the arrow keys as its heading does', async () => {
    fakeStore();
    const list = await mount(GROUPED);
    await list.key('ArrowDown', list.groupSwitch('combat'));
    expect(list.selected()).toMatch(/^Flee below 20 percent/);
    await list.key('ArrowLeft', list.groupSwitch('idle'));
    expect(list.open('g:idle')).toBe(true);
  });
});

// Unsaved changes hold the profile Settings shows while the selection
// moves to a session on another profile, and the save lands on the
// profile it was made on. This runs last, since the profile Settings
// shows stays at module scope.
describe('a draft with unsaved changes', () => {
  const ROWS = [
    {
      id: 1,
      name: null,
      character: 'Tolliver',
      host: 'play.theforsakenlands.com',
      port: 1848,
      tls: false,
      profile: 'default',
      connected: true,
      selected: true,
    },
    {
      id: 2,
      name: null,
      character: 'Orla',
      host: 'play.theforsakenlands.com',
      port: 1825,
      tls: false,
      profile: 'Build',
      connected: true,
      selected: false,
    },
  ];
  const ORLA_SELECTED = [
    { ...ROWS[0], selected: false },
    { ...ROWS[1], selected: true },
  ];

  /** Send every window the rows, as the app does after a step. */
  const sessions = (rows: typeof ROWS) =>
    act(async () => {
      for (const [event, cb] of vi.mocked(listen).mock.calls) {
        if (event === 'vosh://sessions-changed') (cb as (e: unknown) => void)({ payload: rows });
      }
    });

  afterEach(() => {
    vi.mocked(invoke).mockImplementation(() => Promise.resolve());
  });

  it("lands a held Save on Default while Orla's session is selected", async () => {
    vi.mocked(invoke).mockImplementation(((cmd: string) =>
      Promise.resolve(cmd === 'sessions_list' ? ROWS : undefined)) as typeof invoke);
    const loaded: (string | undefined)[] = [];
    const saved: (string | undefined)[] = [];
    const spec: KindSpec<Thing> = {
      ...SPEC,
      load: (profile) => {
        loaded.push(profile);
        return Promise.resolve(things);
      },
      save: (_draft, _written, profile) => {
        saved.push(profile);
        return Promise.resolve();
      },
    };
    const list = await mount(spec);
    await sessions(ROWS);
    await act(async () => detail?.update((t) => ({ ...t, name: `${t.name} again` })));
    loaded.length = 0;

    await sessions(ORLA_SELECTED);
    expect(loaded).toEqual([]);
    await list.click(list.button('Save'));

    expect(saved).toEqual(['default']);
    // The load after the save reads Default, and then Settings follows
    // Orla to Build.
    expect(loaded).toEqual(['default', 'Build']);
  });
});
