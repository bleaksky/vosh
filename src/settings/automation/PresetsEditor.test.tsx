import { act, useState } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { Macro } from '../../ipc/automation';
import type { PresetToggle } from '../../automation/automationRecords';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';

// The preset card of Scripts board 7: Numpad movement's toggle, Adds,
// the Keys row and the note when a macro of yours keeps a key. This
// mounts the card over a fake event bus and a fake macros_list, so the
// macro list store loads, follows each list the backend sends and asks
// again on a profile switch, as it does in the app.

type Handler = (event: { payload: unknown }) => void;
const bus = vi.hoisted(() => ({
  handlers: new Map<string, Set<Handler>>(),
  macros: [] as unknown[],
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (event: string, cb: Handler) => {
    let set = bus.handlers.get(event);
    if (!set) bus.handlers.set(event, (set = new Set()));
    set.add(cb);
    return () => set.delete(cb);
  },
  emit: async () => undefined,
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string) => {
    if (cmd !== 'macros_list') throw new Error(`no fake for ${cmd}`);
    return bus.macros;
  },
}));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let PresetDetail: typeof import('./PresetsEditor').PresetDetail;

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
  ({ createRoot } = await import('react-dom/client'));
  ({ PresetDetail } = await import('./PresetsEditor'));
});

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
});

/** Send `payload` to every listener of `event`, as the backend would,
 *  and let the store's reads settle. */
async function fire(event: string, payload: unknown): Promise<void> {
  await act(async () => {
    for (const cb of bus.handlers.get(event) ?? []) cb({ payload });
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

const mine = (key: string, command: string): Macro => ({ key, command });
const NUMPAD_MOVEMENT = ['Numpad8', 'Numpad6', 'Numpad2', 'Numpad4', 'Numpad9', 'Numpad3'];
const LETTERS = ['n', 'e', 's', 'w', 'u', 'd'];
/** The six macros Numpad movement adds, as the store sends them once
 *  your `rec` keeps Numpad3. */
const INSTALLED: Macro[] = NUMPAD_MOVEMENT.map((key, n) => ({
  key,
  command: LETTERS[n],
  preset: 'numpad_movement',
  ...(key === 'Numpad3' ? { enabled: false } : {}),
}));

async function mount(start: PresetToggle) {
  function Card() {
    const [value, setValue] = useState(start);
    return (
      <PresetDetail
        uid="p1"
        value={value}
        update={(fn) => setValue(fn)}
        fresh={false}
        revealInList={() => {}}
      />
    );
  }
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    root.render(<Card />);
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const rows = () => findAll(container, (el) => el.getAttribute('class') === 'st-row');
  const row = (label: string) =>
    rows().find((r) => findAll(r, isLabel).some((l) => l.textContent === label));
  return {
    /** The value beside a row's label. */
    value: (label: string) => row(label)?.textContent.replace(label, ''),
    /** Each key of the Keys row with what it sends, `8 n`, and `!` after
     *  one that wears the warn ring. */
    keys: () =>
      findAll(container, (el) => hasClass(el, 'st-auto-keypair')).map(
        (pair) =>
          `${findAll(pair, (el) => el.nodeName === 'KBD')[0]?.textContent} ` +
          `${findAll(pair, (el) => hasClass(el, 'st-auto-keysend'))[0]?.textContent}` +
          (hasClass(pair, 'is-warn') ? '!' : ''),
      ),
    /** The warn note that closes the card, or undefined. */
    note: () =>
      findAll(container, (el) => el.getAttribute('class') === 'st-card-note is-warn')[0]
        ?.textContent,
    /** True when the note is the card's last child. */
    noteLast: () => {
      const card = findAll(container, (el) => hasClass(el, 'st-card'))[0];
      const last = card?.childNodes[card.childNodes.length - 1];
      return last instanceof FakeElement && hasClass(last, 'st-card-note');
    },
    /** The preset's switch, as React last set it. */
    on: () => (switchInput() as unknown as { checked: boolean }).checked,
    /** Flip the preset's switch, as a click does. */
    toggle: () =>
      act(async () => {
        const input = switchInput();
        const key = Object.keys(input).find((k) => k.startsWith('__reactProps$'));
        const props = (input as unknown as Record<string, Record<string, (e: unknown) => void>>)[
          key ?? ''
        ];
        props.onChange({
          target: { checked: !(input as unknown as { checked: boolean }).checked },
        });
      }),
  };
  function switchInput(): FakeElement {
    const input = findAll(container, (el) => el.getAttribute('role') === 'switch')[0];
    if (!input) throw new Error('no switch');
    return input;
  }
}

/** Make `list` the macros the store holds. Before the card first mounts
 *  the store has not started, and macros_list answers with it. After,
 *  the backend sends it, as it does after each change. */
async function holding(list: Macro[]): Promise<void> {
  bus.macros = list;
  await fire('vosh://macros-changed', list);
}

const isLabel = (el: FakeElement) => hasClass(el, 'st-row-label');
const hasClass = (el: FakeElement, name: string) =>
  (el.getAttribute('class') ?? '').split(' ').includes(name);

// The tests run in order and share the store, which loads once, on the
// first card's mount.
describe('the Numpad movement card', () => {
  it('loads your macros, lists the six keys in game order and says your macro keeps Numpad3', async () => {
    await holding([mine('F1', 'score'), mine('Numpad3', 'rec'), ...INSTALLED]);
    const card = await mount({ id: 'numpad_movement', enabled: true });
    expect(card.value('Adds')).toBe('6 macros');
    expect(card.keys()).toEqual(['8 n', '6 e', '2 s', '4 w', '9 u', '3 d!']);
    expect(card.note()).toBe(
      'Your macro on Numpad3 keeps the key, so d has none until you move it.',
    );
    expect(card.noteLast()).toBe(true);
  });

  it('keeps the note with the preset off, since it holds either way', async () => {
    await holding([mine('Numpad3', 'rec')]);
    const card = await mount({ id: 'numpad_movement', enabled: true });
    await card.toggle();
    expect(card.on()).toBe(false);
    expect(card.keys()).toEqual(['8 n', '6 e', '2 s', '4 w', '9 u', '3 d!']);
    expect(card.note()).toBe(
      'Your macro on Numpad3 keeps the key, so d has none until you move it.',
    );
  });

  it('follows the macros you save and the profile you switch to', async () => {
    await holding([mine('F1', 'score')]);
    const card = await mount({ id: 'numpad_movement', enabled: false });
    expect(card.keys()).toEqual(['8 n', '6 e', '2 s', '4 w', '9 u', '3 d']);
    expect(card.note()).toBeUndefined();

    // Macros saves your rec on Numpad3 and gate on Numpad9, and the
    // backend sends the new list. A preset macro on a key never keeps it.
    await fire('vosh://macros-changed', [
      mine('Numpad3', 'rec'),
      mine('Numpad9', 'gate'),
      { key: 'Numpad8', command: 'n', preset: 'numpad_movement' },
    ]);
    expect(card.keys()).toEqual(['8 n', '6 e', '2 s', '4 w', '9 u!', '3 d!']);
    expect(card.note()).toBe(
      'Your macros on Numpad9 and Numpad3 keep their keys, so u and d have none until you move them.',
    );

    // Another profile, with no macro on the numpad.
    bus.macros = [mine('F2', 'flee')];
    await fire('vosh://profile-switched', 'Maren');
    expect(card.keys()).toEqual(['8 n', '6 e', '2 s', '4 w', '9 u', '3 d']);
    expect(card.note()).toBeUndefined();
  });

  it('keeps the card of a trigger preset as it was', async () => {
    const card = await mount({ id: 'room_and_time', enabled: true });
    expect(card.value('Adds')).toBe('6 triggers');
    expect(card.keys()).toEqual([]);
    expect(card.value('Keys')).toBeUndefined();
    expect(card.note()).toBeUndefined();
  });
});
