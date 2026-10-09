import { act } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { Macro } from '../../ipc/automation';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';
import keptKeys from '../../../fixtures/macros/kept-keys.json';

// The Macros list: the six macros Numpad movement adds under From
// presets, the warn ring on your macro that keeps Numpad3, and the card
// of each side of that clash. This mounts the editor over a fake
// macros_list and drives it through the handlers React keeps on each
// element, since this DOM sends no events.

/** The store with a clash, the first case Rust holds to
 *  hold_taken_keys. Your rec keeps Numpad3, so the preset's d on it is
 *  held off. */
const B7: Macro[] = keptKeys.cases[0].macros;

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  emit: vi.fn(() => Promise.resolve()),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string) => Promise.resolve(cmd === 'macros_list' ? B7 : [])),
}));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let MacrosEditor: typeof import('./MacrosEditor').MacrosEditor;

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
  ({ createRoot } = await import('react-dom/client'));
  ({ MacrosEditor } = await import('./MacrosEditor'));
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

async function mount() {
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    root.render(
      <MacrosEditor json={false} onJson={() => {}} onDirty={() => {}} onError={() => {}} />,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const rows = () => findAll(container, (el) => el.hasAttribute('data-uid'));
  /** A row's key and command, like `Numpad3rec`. */
  const shows = (row: FakeElement) =>
    findAll(row, (el) => hasClass(el, 'st-auto-row-name') || hasClass(el, 'st-auto-row-meta'))
      .map((el) => el.textContent)
      .join('');
  const row = (name: string) => {
    const found = rows().find((el) => shows(el) === name);
    if (!found) throw new Error(`no row ${name}`);
    return found;
  };
  const card = () => findAll(container, (el) => hasClass(el, 'st-auto-card'))[0];
  const cardRow = (label: string) =>
    findAll(card(), (el) => el.getAttribute('class') === 'st-row').find(
      (r) => findAll(r, (el) => hasClass(el, 'st-row-label'))[0]?.textContent === label,
    );
  return {
    /** Each row as `key command`, with `!` after one that wears the
     *  warn ring and `.` after one that is off. */
    rows: () =>
      rows().map(
        (el) =>
          shows(el) +
          (hasClass(el, 'is-warn') ? '!' : '') +
          (findAll(el, (dot) => hasClass(dot, 'st-auto-dot') && hasClass(dot, 'is-off')).length
            ? '.'
            : ''),
      ),
    /** The note a reader hears for a row, from its description. */
    rowNote: (name: string) => {
      const id = row(name).getAttribute('aria-describedby');
      return id ? findAll(container, (el) => el.getAttribute('id') === id)[0]?.textContent : null;
    },
    headings: () =>
      findAll(container, (el) => hasClass(el, 'st-auto-fold-name')).map((el) => el.textContent),
    pick: (name: string) =>
      act(async () => {
        on(row(name)).onClick();
      }),
    /** The card's notes, the warn ones marked `!`. */
    notes: () =>
      findAll(card(), (el) => hasClass(el, 'st-card-note')).map(
        (el) => (hasClass(el, 'is-warn') ? '! ' : '') + el.textContent,
      ),
    /** Whether the control of a card row takes no input. */
    disabled: (label: string) => {
      const row = cardRow(label);
      const input = row && findAll(row, (el) => el.nodeName === 'INPUT')[0];
      if (!input) throw new Error(`no control for ${label}`);
      return Boolean(on(input).disabled);
    },
    deletes: () =>
      findAll(container, (el) => el.nodeName === 'BUTTON' && el.textContent === 'Delete macro')
        .length > 0,
  };
}

const WANTS =
  'Numpad movement also wants Numpad3, for d. Your macro keeps the key, so d has no key until you move this one.';

describe('the Macros list with Numpad movement on', () => {
  it('lists the six preset macros under From presets in the order stored', async () => {
    const list = await mount();
    expect(list.headings()).toEqual(['info', 'From presets']);
    expect(list.rows()).toEqual([
      'F2flee.',
      'Numpad3rec!',
      'F1score',
      'Numpad8n',
      'Numpad6e',
      'Numpad2s',
      'Numpad4w',
      'Numpad9u',
      'Numpad3d.',
    ]);
  });

  it('rings your macro that keeps Numpad3 and says why, and leaves the held d plain', async () => {
    const list = await mount();
    expect(list.rowNote('Numpad3rec')).toBe(WANTS);
    expect(list.rowNote('Numpad3d')).toBeNull();
    expect(list.rowNote('F2flee')).toBeNull();
  });

  it('says on the card of your macro which preset macro is held off', async () => {
    const list = await mount();
    await list.pick('Numpad3rec');
    expect(list.notes()).toEqual([`! ${WANTS}`]);
    expect(list.disabled('Key')).toBe(false);
    expect(list.disabled('Command')).toBe(false);
    expect(list.disabled('Enabled')).toBe(false);
    expect(list.deletes()).toBe(true);
  });

  it('changes only the group of a preset macro, and says yours keeps the key of the held one', async () => {
    const list = await mount();
    await list.pick('Numpad3d');
    expect(list.notes()).toEqual([
      '! Your macro on Numpad3 keeps the key, so d has none until you move it.',
      'This macro comes from a preset, so only its group changes here. Turn the preset off under Presets to remove it.',
    ]);
    expect(list.disabled('Key')).toBe(true);
    expect(list.disabled('Command')).toBe(true);
    expect(list.disabled('Enabled')).toBe(true);
    expect(list.disabled('Group')).toBe(false);
    expect(list.deletes()).toBe(false);

    // One that holds its key has no warn note.
    await list.pick('Numpad8n');
    expect(list.notes()).toEqual([
      'This macro comes from a preset, so only its group changes here. Turn the preset off under Presets to remove it.',
    ]);
  });
});
