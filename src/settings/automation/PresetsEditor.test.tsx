import { act, useState } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { Macro } from '../../ipc/automation';
import type { PresetToggle } from '../../automation/automationRecords';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';

// The preset card of Scripts board 7: Numpad movement's toggle, Adds,
// the Keys row and the note when a macro of yours keeps a key. This
// mounts the card over a fake event bus and a fake macros_list, so the
// macro list store loads, follows each list the backend sends and asks
// again on a profile switch, as it does in the app. Then the Alerts
// category of board 2 of the Alerts review, in the whole editor over a
// fake profile.

type Handler = (event: { payload: unknown }) => void;
const bus = vi.hoisted(() => ({
  handlers: new Map<string, Set<Handler>>(),
  macros: [] as unknown[],
  /** What ui_get_config and alert_presets_get answer. */
  enabled: [] as string[],
  alerts: {} as unknown,
  /** Every command the editor sent, with its arguments, but the reads. */
  calls: [] as [string, unknown][],
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
  invoke: async (cmd: string, args?: unknown) => {
    if (cmd === 'macros_list') return bus.macros;
    if (cmd === 'ui_get_config') return { enabled_presets: bus.enabled };
    if (cmd === 'alert_presets_get') return bus.alerts;
    if (['alert_presets_set', 'ui_set_fields', 'presets_install', 'presets_remove'].includes(cmd)) {
      bus.calls.push([cmd, args]);
      // Keep what a set writes, so a load after Save reads it back.
      if (cmd === 'alert_presets_set') {
        const { id, alert } = args as { id: string; alert: unknown };
        const table = (bus.alerts as { alerts: Record<string, unknown> }).alerts;
        if (alert) table[id] = alert;
        else delete table[id];
      }
      return 0;
    }
    throw new Error(`no fake for ${cmd}`);
  },
}));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let PresetDetail: typeof import('./PresetsEditor').PresetDetail;
let PresetsEditor: typeof import('./PresetsEditor').PresetsEditor;

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
  ({ PresetDetail, PresetsEditor } = await import('./PresetsEditor'));
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

const ALERT_IDS = [
  'alert_tells',
  'alert_name',
  'alert_attacked',
  'alert_low_health',
  'alert_connection',
];

/** The handlers React keeps on an element. */
function reactProps(el: FakeElement): Record<string, (e?: unknown) => void> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, (e?: unknown) => void>>)[key];
}

/** The whole Presets editor over a profile with `enabled` stored and
 *  the alert presets `on` with `alerts` for parts. */
async function mountEditor(enabled: string[], on: string[], alerts: Record<string, unknown>) {
  bus.enabled = enabled;
  bus.alerts = { ids: ALERT_IDS, on, alerts };
  bus.calls = [];
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const errors: (string | null)[] = [];
  await act(async () => {
    root.render(
      <PresetsEditor
        setConfig={() => {}}
        onDirty={() => {}}
        onError={(e) => errors.push(e)}
        profileScoped={false}
      />,
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
  });
  const click = (el: FakeElement | undefined) =>
    act(async () => {
      if (!el) throw new Error('nothing to click');
      reactProps(el).onClick({ currentTarget: el, preventDefault() {} });
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  const card = () => findAll(container, (el) => hasClass(el, 'st-auto-card'))[0];
  return {
    errors,
    /** The headings of the list, in order. */
    headings: () =>
      findAll(container, (el) => hasClass(el, 'st-auto-fold-name')).map((el) => el.textContent),
    pick: (name: string) =>
      click(
        findAll(
          container,
          (el) => el.hasAttribute('data-uid') && el.textContent.startsWith(name),
        )[0],
      ),
    click: (text: string) =>
      click(
        findAll(
          card() ?? container,
          (el) => el.nodeName === 'BUTTON' && el.textContent === text,
        )[0],
      ),
    save: () =>
      click(findAll(container, (el) => el.nodeName === 'BUTTON' && el.textContent === 'Save')[0]),
    /** The labels of the card's rows, in order. */
    rows: () => findAll(card(), isLabel).map((el) => el.textContent),
    value: (label: string) => {
      const row = findAll(card(), (el) => el.getAttribute('class') === 'st-row').find((r) =>
        findAll(r, isLabel).some((l) => l.textContent === label),
      );
      return row?.textContent.replace(label, '');
    },
    /** The parts of the Alert row, `+` before a pressed one. */
    parts: () =>
      findAll(
        findAll(card(), (el) => el.getAttribute('aria-label') === 'Alert with')[0],
        (el) => el.nodeName === 'BUTTON',
      ).map((b) => `${b.getAttribute('aria-pressed') === 'true' ? '+' : ''}${b.textContent}`),
    status: () =>
      findAll(container, (el) => el.getAttribute('class') === 'st-savebar-status')[0]?.textContent,
  };
}

const TELLS = {
  banner: true,
  sound: 'chime',
  attention: 'once',
  background: true,
  words: false,
};

describe('the Alerts category', () => {
  it('lists the five first under Alerts and fills each card from the profile', async () => {
    const editor = await mountEditor(
      ['alert_tells', 'alert_attacked'],
      ['alert_tells', 'alert_attacked'],
      { alert_tells: TELLS },
    );
    expect(editor.headings()[0]).toBe('Alerts');
    await editor.pick('Tells you get');
    expect(editor.parts()).toEqual(['+Banner', '+Sound', '+Bounce']);
    expect(editor.rows()).toEqual([
      'Tells you get',
      'Listens to',
      'Alert',
      'Sound',
      'Bounce',
      'Banner shows',
      'Only while you are not looking at its session',
      'Adds',
    ]);
    expect(editor.value('Listens to')).toBe("The game's word that a tell reached you");
    expect(editor.value('Adds')).toBe('1 alert');

    // A preset the [alerts] table leaves out posts a banner alone.
    await editor.pick('Your name');
    expect(editor.parts()).toEqual(['+Banner', 'Sound', 'Bounce']);
    expect(editor.rows()).toEqual([
      'Your name',
      'Listens to',
      'Alert',
      'Banner shows',
      'Only while you are not looking at its session',
      'Adds',
    ]);
  });

  it('shows no Banner shows row for Low health, whose banner holds no words', async () => {
    const editor = await mountEditor([], [], {});
    await editor.pick('Low health');
    expect(editor.parts()).toEqual(['+Banner', 'Sound', 'Bounce']);
    expect(editor.rows()).not.toContain('Banner shows');
  });

  it('saves the parts of the preset you changed, then the list', async () => {
    const editor = await mountEditor(['alert_tells'], ['alert_tells'], { alert_tells: TELLS });
    await editor.pick('Tells you get');
    expect(editor.status()).toBe('');
    await editor.click('Bounce');
    expect(editor.parts()).toEqual(['+Banner', '+Sound', 'Bounce']);
    expect(editor.rows()).not.toContain('Bounce');
    expect(editor.status()).toBe('Unsaved changes');

    await editor.pick('Being attacked');
    await editor.click('Sound');
    await editor.click('Sound');
    await editor.save();
    expect(editor.errors.filter(Boolean)).toEqual([]);
    const names = bus.calls.map(([cmd]) => cmd);
    expect(names).toEqual(['alert_presets_set', 'ui_set_fields']);
    expect(bus.calls[0][1]).toEqual({
      id: 'alert_tells',
      alert: { banner: true, sound: 'chime', background: true, words: false },
    });
    const fields = (bus.calls[1][1] as { fields: { field: string; value: string[] }[] }).fields;
    expect(fields).toEqual([{ field: 'enabled_presets', value: ['alert_tells'] }]);
  });

  it('forgets the parts that match the default, and keeps alert ids out of install and remove', async () => {
    const editor = await mountEditor([], [], {});
    await editor.pick('Connection');
    await editor.click('Sound');
    await editor.save();
    await editor.click('Sound');
    await editor.save();
    const sets = bus.calls.filter(([cmd]) => cmd === 'alert_presets_set').map(([, a]) => a);
    expect(sets).toEqual([
      {
        id: 'alert_connection',
        alert: { banner: true, sound: 'chime', background: true, words: false },
      },
      { id: 'alert_connection', alert: null },
    ]);
    const installs = bus.calls.filter(
      ([cmd]) => cmd === 'presets_install' || cmd === 'presets_remove',
    );
    expect(JSON.stringify(installs)).not.toContain('alert_');
  });
});
