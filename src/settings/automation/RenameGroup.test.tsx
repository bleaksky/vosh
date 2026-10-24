import { act } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GroupSwitch, LoadoutHold } from '../../ipc/automation';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';

// Renaming a group from its heading's menu: the field in place of the
// heading, the confirm before a merge into a group you have, the refusal
// when both groups hold one name, and the note the rename leaves. This
// mounts the editor over a fake alias store, as BatchEdit.test.tsx does,
// whose groups_rename moves the aliases as the store would.

interface StoredAlias {
  name: string;
  expansion: string;
  enabled?: boolean;
  group?: string;
}

const ALIASES: StoredAlias[] = [
  { name: 'ds', expansion: "cast 'detect scry'", group: 'Tolliver' },
  { name: 'res', expansion: '#group Maren off', group: 'Tolliver' },
  { name: 'ds', expansion: "cast 'detect scry'", group: 'Maren' },
  { name: 'heal', expansion: "cast 'heal'", group: 'Maren' },
];

/** What the fake store holds, as aliases_export sends it. */
let stored = '[]';
let switches: GroupSwitch[] = [];
let newHold: LoadoutHold | null = null;

/** The store's rename: asks first for a group you have, refuses a name
 *  both groups hold, and moves the rest. */
function rename(args: Record<string, unknown>) {
  const { from, to, merge } = args as { from: string; to: string; merge: boolean };
  const list = JSON.parse(stored) as StoredAlias[];
  if (list.some((a) => a.group === to) && !merge) return { outcome: 'exists' };
  const both = list.filter(
    (a) => a.group === from && list.some((b) => b.group === to && b.name === a.name),
  );
  if (both.length > 0) {
    throw new Error(
      `${from} and ${to} both have an alias named ${both[0].name}. Rename one of them first.`,
    );
  }
  let items = 0;
  for (const a of list) {
    if (a.group === from) {
      a.group = to;
      items += 1;
    }
  }
  stored = JSON.stringify(list);
  switches = switches.map((s) => (s.name === from ? { ...s, name: to } : s));
  return { outcome: 'renamed', items };
}

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
    if (cmd === 'groups_rename') return Promise.resolve().then(() => rename(args ?? {}));
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
    innerWidth: 880,
    innerHeight: 600,
  });
  // The menus and the dialog measure and compare elements.
  vi.stubGlobal('HTMLElement', FakeElement);
  Object.assign(FakeElement.prototype, {
    select() {},
    getBoundingClientRect: () => ({
      left: 0,
      top: 0,
      right: 200,
      bottom: 30,
      width: 200,
      height: 30,
    }),
    contains(this: FakeElement, other: unknown) {
      let node = other as FakeElement | null;
      while (node) {
        if (node === this) return true;
        node = node.parentNode as FakeElement | null;
      }
      return false;
    },
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
     *  after one the save error names, `*` after the selected one, and
     *  `+` after one checked while picking. */
    rows: () =>
      rows().map(
        (el) =>
          `${groupOf(el)}/${nameOf(el)}` +
          (hasClass(el, 'is-warn') ? '!' : '') +
          (hasClass(el, 'is-error') ? 'x' : '') +
          (el.getAttribute('aria-current') === 'true' ? '*' : '') +
          (el.getAttribute('aria-pressed') === 'true' ? '+' : ''),
      ),
    /** Click a row, with the modifier keys `keys` holds. */
    pick: (group: string, name: string, keys: { ctrlKey?: boolean; shiftKey?: boolean } = {}) =>
      act(async () => {
        const row = rows().find((el) => groupOf(el) === group && nameOf(el) === name);
        if (!row) throw new Error(`no row ${group}/${name}`);
        on(row).onClick(keys);
        await settle();
      }),
    /** Press the button the label `label` names, like Select. */
    press: (label: string) =>
      act(async () => {
        const el = findAll(container, (n) => n.getAttribute('aria-label') === label)[0];
        if (!el) throw new Error(`no ${label} button`);
        on(el).onClick();
        await settle();
      }),
    /** Type into the field the label names, without leaving it. */
    fill: (label: string) => (text: string) =>
      act(async () => on(labelled(label)).onChange({ target: { value: text } })),
    /** The notes on the card that are not warnings. */
    notes: () =>
      findAll(container, (el) => el.getAttribute('class') === 'st-card-note').map(
        (el) => el.textContent,
      ),
    /** Type into the filter over the list. */
    filter: (text: string) =>
      act(async () => {
        const el = findAll(container, (n) => n.getAttribute('aria-label') === 'Filter aliases')[0];
        if (!el) throw new Error('no filter');
        on(el).onChange({ target: { value: text } });
        await settle();
      }),
    /** Right click the heading of `group`. */
    headingMenu: (group: string) =>
      act(async () => {
        const name = findAll(
          container,
          (el) => hasClass(el, 'st-auto-fold-name') && el.textContent === group,
        )[0];
        let row = name?.parentNode as FakeElement | null;
        while (row && !hasClass(row, 'st-auto-headrow')) row = row.parentNode as FakeElement | null;
        if (!row) throw new Error(`no heading ${group}`);
        on(row).onContextMenu({ preventDefault() {}, clientX: 10, clientY: 10 });
        await settle();
      }),
    /** The rows of the open menu, `(off)` after one you cannot choose. */
    menuRows: () =>
      findAll(doc.body, (el) => (el.getAttribute('role') ?? '').startsWith('menuitem')).map(
        (el) => el.textContent + (el.getAttribute('aria-disabled') === 'true' ? ' (off)' : ''),
      ),
    /** Choose the menu row that says `text`. */
    choose: (text: string) =>
      act(async () => {
        const el = findAll(
          doc.body,
          (n) => (n.getAttribute('role') ?? '').startsWith('menuitem') && n.textContent === text,
        )[0];
        if (!el) throw new Error(`no menu row ${text}`);
        on(el).onClick();
        await settle();
      }),
    /** Type a group's new name in its field and press Return. */
    rename: async (group: string, to: string) => {
      const field = () =>
        findAll(container, (n) => n.getAttribute('aria-label') === `New name for ${group}`)[0];
      if (!field()) throw new Error(`no name field for ${group}`);
      // Typing lands before Return reads it, as it does between keys.
      await act(async () => on(field()).onChange({ target: { value: to } }));
      await act(async () => {
        on(field()).onKeyDown({ key: 'Enter', preventDefault() {} });
        await settle();
        await settle();
      });
    },
    /** Press a button anywhere on the page, like one in a dialog. */
    answer: (text: string) =>
      act(async () => {
        const el = findAll(doc.body, (n) => n.nodeName === 'BUTTON' && n.textContent === text)[0];
        if (!el) throw new Error(`no button ${text}`);
        on(el).onClick();
        await settle();
        await settle();
      }),
    /** The title of the open dialog, or null. */
    dialog: () =>
      findAll(
        doc.body,
        (el) => el.getAttribute('role') === 'alertdialog' || el.getAttribute('role') === 'dialog',
      )[0]?.textContent ?? null,
    /** The notes under the group headings. */
    headingNotes: () =>
      findAll(container, (el) => hasClass(el, 'st-auto-groupnote')).map((el) => el.textContent),
    /** How many the card for the picked rows says, or null. */
    title: () =>
      findAll(container, (el) => hasClass(el, 'st-auto-batch-title'))[0]?.textContent ?? null,
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

beforeEach(() => {
  stored = JSON.stringify(ALIASES);
  switches = [
    { name: 'Maren', enabled: true },
    { name: 'Tolliver', enabled: true },
  ];
  newHold = null;
});

describe('renaming a group', () => {
  it('renames a group from its heading and says what still names it', async () => {
    const page = await mount();
    await page.headingMenu('Maren');
    expect(page.menuRows()).toEqual([
      'Rename group…',
      'Select all in Maren',
      'Move all to no group',
    ]);
    await page.choose('Rename group…');
    await page.rename('Maren', 'Healer');
    expect(storedAliases().map((a) => `${a.group ?? ''}/${a.name}`)).toEqual([
      'Tolliver/ds',
      'Tolliver/res',
      'Healer/ds',
      'Healer/heal',
    ]);
    expect(page.rows().map((r) => r.replace(/[!*+x]/g, ''))).toEqual([
      'Healer/ds',
      'Healer/heal',
      'Tolliver/ds',
      'Tolliver/res',
    ]);
    expect(page.headingNotes()).toEqual(['Renamed from Maren. 1 alias still says Maren. Show it']);
  });

  it('asks before it merges into a group you have, and shows a refusal', async () => {
    const page = await mount();
    await page.headingMenu('Maren');
    await page.choose('Rename group…');
    await page.rename('Maren', 'Tolliver');
    expect(page.dialog()).toContain('Merge Maren into Tolliver?');
    await page.answer('Merge');
    expect(page.top()).toBe(
      'Maren and Tolliver both have an alias named ds. Rename one of them first.',
    );
    expect(storedAliases().filter((a) => a.group === 'Maren')).toHaveLength(2);
  });

  it('waits for Save or Discard before a rename', async () => {
    const page = await mount();
    await page.pick('Maren', 'heal');
    await page.pick('Tolliver', 'res', { ctrlKey: true });
    await page.click('Turn off');
    await page.headingMenu('Maren');
    expect(page.menuRows()[0]).toBe('Rename group… (off)');
  });

  it('moves a whole group out of its group from the menu', async () => {
    const page = await mount();
    await page.headingMenu('Maren');
    await page.choose('Move all to no group');
    expect(page.rows().map((r) => r.replace(/[!*+x]/g, ''))).toEqual([
      '/ds',
      '/heal',
      'Tolliver/ds',
      'Tolliver/res',
    ]);
  });
});
