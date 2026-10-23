import { act } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { GroupSwitch, LoadoutHold } from '../../ipc/automation';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';

// Picking several aliases at once: Ctrl and Shift clicks, Select and
// Done, and the card that moves, turns, copies and deletes every
// checked alias in one step, each a change to the draft that Save
// writes. This mounts the editor over a fake alias store as
// AliasesEditor.test.tsx does, and drives it through the handlers React
// keeps on each element, since this DOM sends no events.

interface StoredAlias {
  name: string;
  expansion: string;
  enabled?: boolean;
  group?: string;
}

const ALIASES: StoredAlias[] = [
  { name: 'ds', expansion: "cast 'detect scry'", group: 'Tolliver' },
  { name: 'res', expansion: "cast 'resurrect'", group: 'Tolliver' },
  { name: 'ds', expansion: "cast 'detect scry'", group: 'Maren' },
  { name: 'heal', expansion: "cast 'heal'" },
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

describe('picking several aliases', () => {
  // Tolliver's ds wears the warn ring throughout, since Maren's ds, listed
  // first, fires instead. While picking, no row is the open one.
  it('checks the open row and a Ctrl clicked one, and the card counts them', async () => {
    const page = await mount();
    await page.pick('Tolliver', 'ds');
    expect(page.title()).toBeNull();
    await page.pick('Tolliver', 'res', { ctrlKey: true });
    expect(page.rows()).toEqual(['/heal', 'Maren/ds', 'Tolliver/ds!+', 'Tolliver/res+']);
    expect(page.title()).toBe('2 aliases selected');
    // A plain click while picking checks or clears a row too.
    await page.pick('Tolliver', 'ds');
    expect(page.title()).toBe('1 alias selected');
  });

  it('checks the run from the open row with a Shift click', async () => {
    const page = await mount();
    await page.pick('', 'heal');
    await page.pick('Tolliver', 'ds', { shiftKey: true });
    expect(page.rows()).toEqual(['/heal+', 'Maren/ds+', 'Tolliver/ds!+', 'Tolliver/res']);
  });

  it('starts with Select and ends with Done', async () => {
    const page = await mount();
    await page.press('Select several aliases');
    expect(page.title()).toBe('1 alias selected');
    await page.click('Done');
    expect(page.title()).toBeNull();
    expect(page.rows()).toEqual(['/heal*', 'Maren/ds', 'Tolliver/ds!', 'Tolliver/res']);
  });

  it('moves the checked aliases to a new group and saves them there', async () => {
    const page = await mount();
    await page.pick('Tolliver', 'ds');
    await page.pick('Tolliver', 'res', { ctrlKey: true });
    await page.fill('Group')('Orla');
    await page.click('Move');
    expect(page.rows()).toEqual(['/heal', 'Maren/ds', 'Orla/ds!+', 'Orla/res+']);
    await page.click('Save');
    expect(storedAliases().map((a) => `${a.group ?? ''}/${a.name}`)).toEqual([
      'Orla/ds',
      'Orla/res',
      'Maren/ds',
      '/heal',
    ]);
    // What you checked stays checked after the Save.
    expect(page.title()).toBe('2 aliases selected');
  });

  it('says before a move that two aliases would share a name', async () => {
    const page = await mount();
    await page.pick('Tolliver', 'ds');
    await page.pick('Tolliver', 'res', { ctrlKey: true });
    await page.fill('Group')('Maren');
    expect(page.cardNote()).toBe(
      'Maren would then have two aliases named ds. Rename one before you save.',
    );
  });

  it('turns the checked aliases off', async () => {
    const page = await mount();
    await page.pick('Tolliver', 'ds');
    await page.pick('Tolliver', 'res', { ctrlKey: true });
    await page.click('Turn off');
    await page.click('Save');
    expect(
      storedAliases()
        .filter((a) => a.enabled === false)
        .map((a) => a.name),
    ).toEqual(['ds', 'res']);
  });

  it('copies each checked alias below it, and the copies are what is checked', async () => {
    const page = await mount();
    await page.pick('', 'heal');
    await page.pick('Maren', 'ds', { ctrlKey: true });
    await page.click('Duplicate 2 aliases');
    expect(page.rows()).toEqual([
      '/heal',
      '/heal-copy+',
      'Maren/ds',
      'Maren/ds-copy+',
      'Tolliver/ds!',
      'Tolliver/res',
    ]);
  });

  it('deletes the checked aliases, stops picking and opens the next row', async () => {
    const page = await mount();
    await page.pick('Maren', 'ds');
    await page.pick('Tolliver', 'ds', { ctrlKey: true });
    await page.click('Delete 2 aliases');
    expect(page.title()).toBeNull();
    expect(page.rows()).toEqual(['/heal', 'Tolliver/res*']);
  });
});
