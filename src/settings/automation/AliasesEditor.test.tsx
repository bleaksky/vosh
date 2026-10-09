import { act } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GroupSwitch, LoadoutHold } from '../../ipc/automation';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';

// The Aliases list with one alias name in two groups, one group of
// aliases for each character: the warn ring and the note on the alias
// another group covers, the error beside Save with the rows it names
// marked, and the note on a new group the loadouts would turn off. This
// mounts the editor over a fake alias store and drives it through the
// handlers React keeps on each element, since this DOM sends no events.

interface StoredAlias {
  name: string;
  expansion: string;
  enabled?: boolean;
  group?: string;
}

const DS: StoredAlias[] = [
  { name: 'ds', expansion: "cast 'detect scry' tolliver", group: 'Tolliver' },
  { name: 'ds', expansion: "cast 'detect scry' maren", group: 'Maren' },
];

/** What the fake store holds, as aliases_export sends it. */
let stored = '[]';
let switches: GroupSwitch[] = [];
let newHold: LoadoutHold | null = null;

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  emit: vi.fn(() => Promise.resolve()),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: Record<string, unknown>) => {
    if (cmd === 'aliases_export') return Promise.resolve(stored);
    if (cmd === 'aliases_import') {
      stored = String(args?.json);
      return Promise.resolve((JSON.parse(stored) as unknown[]).length);
    }
    if (cmd === 'groups_list') return Promise.resolve(switches);
    if (cmd === 'groups_new_hold') return Promise.resolve(newHold);
    return Promise.resolve(null);
  }),
}));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let AliasesEditor: typeof import('./AliasesEditor').AliasesEditor;

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
    getItem: () => null,
    setItem: () => undefined,
    removeItem: () => undefined,
  });
  vi.stubGlobal('CSS', { escape: (s: string) => s });
  // The list scrolls a row into view through querySelector, which this
  // DOM leaves out. Nothing here scrolls.
  Object.assign(FakeElement.prototype, { querySelector: () => null });
  ({ createRoot } = await import('react-dom/client'));
  ({ AliasesEditor } = await import('./AliasesEditor'));
});

beforeEach(() => {
  stored = JSON.stringify(DS);
  switches = [
    { name: 'Maren', enabled: true },
    { name: 'Tolliver', enabled: true },
  ];
  newHold = null;
});

type Handler = (e?: unknown) => void;

/** The handlers React keeps on an element. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

const hasClass = (el: FakeElement, name: string) =>
  (el.getAttribute('class') ?? '').split(' ').includes(name);

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

async function mount() {
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  /** The last error the page showed at the top, null once it cleared. */
  let top: string | null = null;
  await act(async () => {
    root.render(
      <AliasesEditor
        json={false}
        onJson={() => {}}
        onDirty={() => {}}
        onError={(message) => {
          top = message;
        }}
      />,
    );
    await settle();
  });
  await act(settle);
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const rows = () => findAll(container, (el) => el.hasAttribute('data-uid'));
  /** The group heading a row sits under, or empty for none. */
  const groupOf = (row: FakeElement) => {
    let el = row.parentNode as FakeElement | null;
    while (el && !hasClass(el, 'st-auto-group')) el = el.parentNode as FakeElement | null;
    if (!el) return '';
    const headings = findAll(container, (h) => hasClass(h, 'st-auto-fold'));
    const id = el.getAttribute('id');
    const heading = headings.find((h) => h.getAttribute('aria-controls') === id);
    return heading
      ? (findAll(heading, (n) => hasClass(n, 'st-auto-fold-name'))[0]?.textContent ?? '')
      : '';
  };
  const nameOf = (row: FakeElement) =>
    findAll(row, (el) => hasClass(el, 'st-auto-row-name'))[0]?.textContent ?? '';
  const button = (text: string) =>
    findAll(container, (el) => el.nodeName === 'BUTTON' && el.textContent === text)[0];
  const labelled = (label: string) => {
    const name = findAll(container, (el) => el.nodeName === 'LABEL' && el.textContent === label)[0];
    const id = name?.getAttribute('for') ?? name?.getAttribute('htmlFor');
    const el = findAll(container, (n) => n.getAttribute('id') === id)[0];
    if (!el) throw new Error(`no ${label} control`);
    return el;
  };
  return {
    /** Each row as `group/name`, `!` after one with the warn ring, `x`
     *  after one the save error names, and `*` after the selected one. */
    rows: () =>
      rows().map(
        (el) =>
          `${groupOf(el)}/${nameOf(el)}` +
          (hasClass(el, 'is-warn') ? '!' : '') +
          (hasClass(el, 'is-error') ? 'x' : '') +
          (el.getAttribute('aria-current') === 'true' ? '*' : ''),
      ),
    pick: (group: string, name: string) =>
      act(async () => {
        const row = rows().find((el) => groupOf(el) === group && nameOf(el) === name);
        if (!row) throw new Error(`no row ${group}/${name}`);
        on(row).onClick();
        await settle();
      }),
    click: (text: string) =>
      act(async () => {
        const el = button(text);
        if (!el) throw new Error(`no button ${text}`);
        on(el).onClick({ currentTarget: el, preventDefault() {} });
        await settle();
        await settle();
      }),
    /** Type `text` into the field the label `label` names, and leave it,
     *  as a Group field commits. */
    type: async (label: string, text: string) => {
      await act(async () => on(labelled(label)).onChange({ target: { value: text } }));
      await act(async () => on(labelled(label)).onBlur?.());
    },
    /** The warn note on the card. */
    cardNote: () =>
      findAll(container, (el) => el.getAttribute('class') === 'st-card-note is-warn')[0]
        ?.textContent,
    /** What stopped a Save, beside the save bar. */
    barError: () =>
      findAll(container, (el) => hasClass(el, 'st-savebar-error'))[0]?.textContent ?? null,
    top: () => top,
    /** The note under each group heading. */
    groupNotes: () =>
      findAll(container, (el) => hasClass(el, 'st-auto-groupnote')).map((el) => el.textContent),
  };
}

const storedAliases = () => JSON.parse(stored) as StoredAlias[];

describe('one alias name in two groups', () => {
  it('rings the covered alias and names the one that fires', async () => {
    const page = await mount();
    // Settings lists Maren first, so Tolliver's ds is the covered one.
    expect(page.rows()).toEqual(['Maren/ds*', 'Tolliver/ds!']);
    expect(page.cardNote()).toBeUndefined();
    await page.pick('Tolliver', 'ds');
    expect(page.cardNote()).toBe('Maren’s ds fires instead while both groups are on.');
  });

  it('says when the covering group is off', async () => {
    switches = [
      { name: 'Maren', enabled: false },
      { name: 'Tolliver', enabled: true },
    ];
    const page = await mount();
    await page.pick('Tolliver', 'ds');
    expect(page.cardNote()).toBe('Maren’s ds fires instead whenever the Maren group is on.');
  });

  it('saves a second ds in a third group', async () => {
    const page = await mount();
    await page.click('New alias');
    await page.type('Name', 'ds');
    await page.type('Group', 'Orla');
    await page.type('Expansion', "cast 'detect scry' orla");
    await page.click('Save');
    expect(page.barError()).toBeNull();
    expect(storedAliases().map((a) => a.group)).toEqual(['Tolliver', 'Maren', 'Orla']);
  });
});

describe('a Save the list stops', () => {
  it('shows the error beside Save, marks the rows, and selects the first', async () => {
    const page = await mount();
    await page.click('New alias');
    await page.type('Name', 'ds');
    await page.type('Group', 'Tolliver');
    await page.click('Save');
    expect(page.barError()).toBe(
      'Tolliver has two aliases named “ds”. Rename one or move it to another group.',
    );
    expect(page.top()).toBeNull();
    expect(page.rows()).toEqual(['Maren/ds', 'Tolliver/ds!x*', 'Tolliver/ds!x']);
    expect(storedAliases()).toEqual(DS);

    // Moving the new one to its own group clears the error and the marks.
    await page.pick('Tolliver', 'ds');
    const rows = page.rows();
    expect(rows[1]).toContain('x');
    await page.type('Group', 'Orla');
    expect(page.barError()).toBeNull();
    expect(page.rows().some((row) => row.includes('x'))).toBe(false);
  });

  it('refuses a name with a space inside and trims one with a space around it', async () => {
    const page = await mount();
    await page.click('New alias');
    await page.type('Name', 'd s');
    await page.click('Save');
    expect(page.barError()).toBe(
      'An alias name is one word, so “d s” won’t work. Take out the space.',
    );
    expect(page.rows()).toContain('/d sx*');

    await page.type('Name', ' res ');
    expect(page.barError()).toBeNull();
    await page.click('Save');
    expect(storedAliases().map((a) => a.name)).toEqual(['ds', 'ds', 'res']);
  });

  it('asks for a name', async () => {
    const page = await mount();
    await page.click('New alias');
    await page.click('Save');
    expect(page.barError()).toBe('This alias needs a name before you can save.');
  });
});

describe('a new group the loadouts would turn off', () => {
  it('says so on its heading before you save', async () => {
    newHold = { on: false, by: ['Tolliver'] };
    const page = await mount();
    expect(page.groupNotes()).toEqual([]);
    await page.click('New alias');
    await page.type('Name', 'res');
    await page.type('Group', 'Orla');
    expect(page.groupNotes()).toEqual([
      'The Tolliver loadout doesn’t list this new group, so it goes off when you next launch Vosh, switch profiles, or save Loadouts. Add it to a loadout to keep it on.',
    ]);
  });
});
