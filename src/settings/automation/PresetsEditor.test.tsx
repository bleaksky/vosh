import { act, useState } from 'react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { Macro } from '../../ipc/automation';
import { normalizeUiConfig } from '../../ipc/uiConfig';
import type { PresetToggle } from '../../automation/automationRecords';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';

// The preset card: Numpad movement's toggle, Adds, the Keys row and the
// note when a macro of yours keeps a key. This mounts the card over a
// fake event bus and a fake macros_list, so the macro list store loads,
// follows each list the backend sends and asks again on a profile
// switch, as it does in the app. Then the Alerts category, in the whole
// editor over a fake profile, and the ask before the first banner with
// the warn ring and the note.

type Handler = (event: { payload: unknown }) => void;
const bus = vi.hoisted(() => ({
  handlers: new Map<string, Set<Handler>>(),
  macros: [] as unknown[],
  /** What ui_get_config and alert_presets_get answer. */
  enabled: [] as string[],
  alerts: {} as unknown,
  /** What preset_edits_get answers, by preset id. */
  edits: {} as Record<string, unknown>,
  /** The triggers triggers_export answers and triggers_import writes. */
  stored: [] as { name: string; group?: string }[],
  /** What alerts_permission answers, and alerts_ask_permission after
   *  you choose. */
  permission: 'granted' as string,
  answer: 'granted' as string,
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
    if (cmd === 'alerts_permission') return bus.permission;
    if (cmd === 'alerts_ask_permission' || cmd === 'alerts_open_settings') {
      bus.calls.push([cmd, args]);
      return cmd === 'alerts_ask_permission' ? bus.answer : null;
    }
    if (cmd === 'preset_edits_get') return bus.edits;
    if (cmd === 'preset_edits_set') {
      bus.calls.push([cmd, args]);
      return null;
    }
    if (cmd === 'triggers_list') return [];
    if (cmd === 'triggers_export') return JSON.stringify(bus.stored);
    if (cmd === 'triggers_import') {
      bus.calls.push([cmd, args]);
      bus.stored = JSON.parse((args as { json: string }).json) as typeof bus.stored;
      return bus.stored.length;
    }
    if (cmd === 'presets_enabled_set') {
      bus.calls.push([cmd, args]);
      // Land the switches on the list as it stands, as switch_presets does.
      for (const { id, on } of (args as { changes: { id: string; on: boolean }[] }).changes) {
        bus.enabled = on
          ? [...bus.enabled.filter((e) => e !== id), id]
          : bus.enabled.filter((e) => e !== id);
      }
      return { installed: 0, removed: [] };
    }
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

// The ask as its props draw it. Its focus trap needs a real DOM, and
// ConfirmDialog.test.tsx checks its markup.
vi.mock('../../ui/ConfirmDialog', async () => {
  const { createElement: h } = await import('react');
  return {
    ConfirmDialog: (p: {
      title: string;
      body: string;
      confirmLabel: string;
      cancelLabel?: string;
      tone?: string;
      onConfirm: () => void;
      onCancel: () => void;
    }) =>
      h(
        'div',
        { className: 'ov-confirm', 'data-tone': p.tone },
        h('h2', null, p.title),
        h('p', null, p.body),
        h('button', { type: 'button', onClick: p.onCancel }, p.cancelLabel ?? 'Cancel'),
        h('button', { type: 'button', onClick: p.onConfirm }, p.confirmLabel),
      ),
  };
});

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
  // A link scrolls the row it opens into view, which the fake page has
  // no layout for.
  vi.stubGlobal('CSS', { escape: (s: string) => s });
  Object.assign(FakeElement.prototype, { querySelector: () => null });
  // The samples follow the theme, which the page marks on its root.
  vi.stubGlobal(
    'MutationObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
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
        onOpenTriggers={() => {}}
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

const CONFIG = normalizeUiConfig({
  theme: 'obsidian-ember',
  auto_update: false,
  font_family: 'Menlo',
  font_size: 14,
  tracked_affects: [],
  enabled_presets: [],
});

/** The whole Presets editor over a profile with `enabled` stored and
 *  the alert presets `on` with `alerts` for parts. */
async function mountEditor(
  enabled: string[],
  on: string[],
  alerts: Record<string, unknown>,
  permission = 'granted',
  selectPreset: { key: string; seq: number } | null = null,
  edits: Record<string, unknown> = {},
) {
  bus.edits = edits;
  bus.permission = permission;
  bus.enabled = enabled;
  bus.alerts = { ids: ALERT_IDS, on, alerts };
  bus.calls = [];
  bus.stored = [];
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const errors: (string | null)[] = [];
  const opened: unknown[] = [];
  await act(async () => {
    root.render(
      <PresetsEditor
        onOpenTriggers={(to) => opened.push(to)}
        config={CONFIG}
        setConfig={() => {}}
        pathB={false}
        selectPreset={selectPreset}
        onDirty={() => {}}
        onError={(e) => errors.push(e)}
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
    /** Where each link of a card opened Triggers. */
    opened,
    /** Press the button that says `text` anywhere on the page. */
    press: (text: string) =>
      click(findAll(container, (el) => el.nodeName === 'BUTTON' && el.textContent === text)[0]),
    /** The links of the card, by their text. */
    links: () => findAll(card(), (el) => hasClass(el, 'st-auto-link')).map((el) => el.textContent),
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
    /** Each row of the list by name, with `+` for a suggested ring and
     *  its anchor, and whether it is the one selected. */
    row: (name: string) => {
      const row = findAll(
        container,
        (el) => el.hasAttribute('data-uid') && el.textContent.startsWith(name),
      )[0];
      const dot = findAll(row, (el) => hasClass(el, 'st-auto-dot'))[0];
      return {
        ...(findAll(row, (el) => hasClass(el, 'st-auto-mark')).length > 0 ? { edited: true } : {}),
        suggested: hasClass(dot, 'is-accent'),
        anchor: row.getAttribute('data-st-anchor'),
        selected: row.getAttribute('aria-current') === 'true',
      };
    },
    /** Each line of the card's Looks like sample, with `|` after it for
     *  each bar that stands in for words. */
    sample: () =>
      findAll(card(), (el) => hasClass(el, 'st-auto-sample-line')).map(
        (line) =>
          line.textContent +
          '|'.repeat(findAll(line, (el) => hasClass(el, 'st-auto-sample-bar')).length),
      ),
    /** What a reader hears with the warn ring of the list row named
     *  `name`, or null while it wears none. */
    ring: (name: string) => {
      const row = findAll(
        container,
        (el) => el.hasAttribute('data-uid') && el.textContent.startsWith(name),
      )[0];
      if (!hasClass(row, 'is-warn')) return null;
      const id = row.getAttribute('aria-describedby');
      return findAll(container, (el) => el.getAttribute('id') === id)[0]?.textContent ?? '';
    },
    /** The labels of the card's rows, in order. */
    rows: () => findAll(card(), isLabel).map((el) => el.textContent),
    value: (label: string) => {
      const row = findAll(card(), (el) => el.getAttribute('class') === 'st-row').find((r) =>
        findAll(r, isLabel).some((l) => l.textContent === label),
      );
      return row?.textContent.replace(label, '');
    },
    /** Each swatch of the Colors block: its label, then what its field
     *  shows, a placeholder in parentheses, and the line under it. */
    swatches: () =>
      findAll(card(), (el) => hasClass(el, 'st-color-cell')).map((cell) => {
        const label = findAll(cell, (el) => hasClass(el, 'st-color-cell-label'))[0].textContent;
        const text = findAll(cell, (el) => hasClass(el, 'st-color-text'))[0];
        const select = findAll(cell, (el) => el.nodeName === 'SELECT')[0];
        const under = findAll(cell, (el) => hasClass(el, 'st-auto-under'))[0];
        const shown = select
          ? `[${String(reactProps(select).value)}]`
          : (text as unknown as { value: string }).value || `(${text.getAttribute('placeholder')})`;
        return [label, shown, ...(under ? [under.textContent] : [])].join(' ');
      }),
    /** Type `text` in the swatch labeled `label` and leave the field. */
    typeColor: (label: string, text: string) =>
      act(async () => {
        const cell = findAll(card(), (el) => hasClass(el, 'st-color-cell')).find(
          (c) => findAll(c, (el) => hasClass(el, 'st-color-cell-label'))[0].textContent === label,
        );
        const input = findAll(cell!, (el) => hasClass(el, 'st-color-text'))[0];
        const target = { value: text };
        reactProps(input).onFocus();
        reactProps(input).onChange({ target, currentTarget: target });
        reactProps(input).onBlur({ target, currentTarget: target });
        await new Promise((resolve) => setTimeout(resolve, 0));
      }),
    /** The parts of the Alert row, `+` before a pressed one. */
    parts: () =>
      findAll(
        findAll(card(), (el) => el.getAttribute('aria-label') === 'Alert with')[0],
        (el) => el.nodeName === 'BUTTON',
      ).map((b) => `${b.getAttribute('aria-pressed') === 'true' ? '+' : ''}${b.textContent}`),
    status: () =>
      findAll(container, (el) => el.getAttribute('class') === 'st-savebar-status')[0]?.textContent,
    /** The ask's tone, then its title, body and buttons, or undefined. */
    ask: () => {
      const card = findAll(container, (el) => hasClass(el, 'ov-confirm'))[0];
      return (
        card && [
          card.getAttribute('data-tone'),
          ...findAll(card, (el) => ['H2', 'P', 'BUTTON'].includes(el.nodeName)).map(
            (el) => el.textContent,
          ),
        ]
      );
    },
    /** The Banner part, with its warn ring and its title. */
    banner: () => {
      const b = findAll(card(), (el) => el.nodeName === 'BUTTON' && el.textContent === 'Banner')[0];
      return { warn: hasClass(b, 'is-warn'), title: b.getAttribute('title') };
    },
    /** The warn note the card opens with, or undefined. */
    note: () =>
      findAll(card(), (el) => el.getAttribute('class') === 'st-card-note is-warn')[0]?.textContent,
    /** Flip the preset's switch, as a click does. */
    toggle: () =>
      act(async () => {
        const input = findAll(card(), (el) => el.getAttribute('role') === 'switch')[0];
        reactProps(input).onChange({
          target: { checked: !(input as unknown as { checked: boolean }).checked },
        });
        await new Promise((resolve) => setTimeout(resolve, 0));
      }),
    /** Whether the preset's switch reads on. */
    on: () =>
      (
        findAll(card(), (el) => el.getAttribute('role') === 'switch')[0] as unknown as {
          checked: boolean;
        }
      ).checked,
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
    expect(editor.value('Listens to')).toBe('Comm.Channel, tell, received');
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

  it('saves only the parts of the preset you changed', async () => {
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
    expect(names).toEqual(['alert_presets_set']);
    expect(bus.calls[0][1]).toEqual({
      id: 'alert_tells',
      alert: { banner: true, sound: 'chime', background: true, words: false },
    });
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

// Every preset off, the suggestions ringed, and the card with Looks
// like and Suggested.
describe('the Presets page of First Run board 4', () => {
  it('rings each suggestion for your world while it is off', async () => {
    const editor = await mountEditor(['sent_tells'], [], {});
    for (const name of [
      'Cures and heals',
      'Your damage verbs',
      'Damage to you',
      'Gold, experience, and levels',
      'Room, time and weather colors',
    ]) {
      expect(editor.row(name).suggested, name).toBe(true);
    }
    for (const name of [
      'Parries, dodges, and blocks',
      'Herb labels',
      'Numpad movement',
      'Tells you get',
    ]) {
      expect(editor.row(name).suggested, name).toBe(false);
    }
    // Once on, it takes the green dot like any other.
    expect(editor.row('Tells you send').suggested).toBe(false);
  });

  it('reads the card as description, Looks like, Colors, Suggested and Adds', async () => {
    const editor = await mountEditor(['none'], [], {});
    await editor.pick('Your damage verbs');
    expect(editor.rows()).toEqual([
      'Your damage verbs',
      'Looks like',
      'Colors',
      'Suggested',
      'Adds',
    ]);
    expect(editor.sample()).toEqual(['You do UNSPEAKABLE things to !|']);
    expect(editor.value('Suggested')).toBe('For The Forsaken Lands');
    expect(editor.value('Adds')).toBe('2 triggers');

    await editor.pick('Herb labels');
    expect(editor.rows()).toEqual(['Herb labels', 'Looks like', 'Colors', 'Adds']);
    await editor.pick('Numpad movement');
    expect(editor.rows()).toEqual(['Numpad movement', 'Adds', 'Keys']);
  });

  it('draws the words a tell quotes as a bar', async () => {
    const editor = await mountEditor(['none'], [], {});
    await editor.pick('Tells you send');
    expect(editor.sample()).toEqual(["You tell Tolliver ''|"]);
  });

  it('draws the attacker and the attack that hit you as bars', async () => {
    const editor = await mountEditor(['none'], [], {});
    await editor.pick('Damage to you');
    expect(editor.sample()).toEqual(['  decimates you!||', '  misses you.||']);
  });

  it('draws the number and the skill a gain names as bars', async () => {
    const editor = await mountEditor(['none'], [], {});
    await editor.pick('Gold, experience, and levels');
    expect(editor.sample()).toEqual([
      'You receive  experience points.|',
      'You have become better at !|',
    ]);
  });

  it('opens on the preset a link names, by its anchor', async () => {
    const editor = await mountEditor(['none'], [], {}, 'granted', { key: 'herb_labels', seq: 1 });
    expect(editor.row('Herb labels')).toEqual({
      suggested: false,
      anchor: 'presets:herb_labels',
      selected: true,
    });
    expect(editor.rows()[0]).toBe('Herb labels');
  });
});

const LILAC = { disarm_buff_fade: { colors: { line: { value: '#c3a6ff', was: 'fg:178' } } } };

/** What Save sent, but the removes of the presets that are off. */
const sentCalls = () => bus.calls.filter(([cmd]) => cmd !== 'presets_remove');

// A preset's colors on its card, one swatch for each.
describe('the Colors block', () => {
  it('shows the preset color in each empty swatch and Back to under one you changed', async () => {
    const editor = await mountEditor(['disarm_buff_fade'], [], {}, 'granted', null, LILAC);
    await editor.pick('Disarms and fading buffs');
    expect(editor.swatches()).toEqual(['The ## mark (Theme red)', 'The line #c3a6ff Back to 178']);

    await editor.pick('Damage to you');
    expect(editor.swatches()).toEqual([
      'The rest of the line (244, #808080)',
      'The damage verb (210, #ff8787)',
      'A miss (152, #afd7d7)',
    ]);
  });

  it('gives a color in a Highlight the theme sixteen and a template any color', async () => {
    const editor = await mountEditor(['none'], [], {});
    await editor.pick('Room, time and weather colors');
    expect(editor.swatches()).toEqual([
      'Exits [green]',
      'What is in the room [yellow]',
      'Your target [bright_red]',
      'Time of day [blue]',
      'Weather change (#8fa7d9)',
      'WiZNET tag [magenta]',
    ]);
    expect(findAll(doc.body, (el) => el.nodeName === 'OPTION').length).toBe(16 * 5);
  });

  it('puts the preset color back with one press, and Save drops your row', async () => {
    const editor = await mountEditor(['disarm_buff_fade'], [], {}, 'granted', null, LILAC);
    await editor.pick('Disarms and fading buffs');
    await editor.click('Back to 178');
    expect(editor.swatches()).toEqual(['The ## mark (Theme red)', 'The line (178, #d7af00)']);
    expect(editor.status()).toBe('Unsaved changes');

    await editor.save();
    expect(sentCalls().map(([cmd]) => cmd)).toEqual(['preset_edits_set', 'presets_install']);
    expect(sentCalls()[0][1]).toEqual({
      id: 'disarm_buff_fade',
      edits: { colors: { line: { value: 'fg:178', was: 'fg:178' } } },
      profile: undefined,
    });
  });

  it('saves a color you type, then runs the plan in it', async () => {
    const editor = await mountEditor(['disarm_buff_fade'], [], {});
    await editor.pick('Disarms and fading buffs');
    await editor.typeColor('The line', '#c3a6ff');
    expect(editor.swatches()[1]).toBe('The line #c3a6ff Back to 178');

    await editor.save();
    expect(sentCalls()[0][1]).toEqual({
      id: 'disarm_buff_fade',
      edits: { colors: { line: { value: '#c3a6ff', was: 'fg:178' } } },
      profile: undefined,
    });
    // The fake keeps no edits, so the plan builds the preset as it ships.
    expect(sentCalls()[1][0]).toBe('presets_install');
  });

  // A fix to the line color Orla changed.
  const FIXED = {
    disarm_buff_fade: { colors: { line: { value: '#c3a6ff', was: 'fg:172', seen: 'fg:178' } } },
  };

  it('rings a swatch a fix changed, and the preset in the list, on or off', async () => {
    const editor = await mountEditor(['none'], [], {}, 'granted', null, FIXED);
    expect(editor.ring('Disarms and fading buffs')).toBe(
      'A fix to this preset changed a row you edited.',
    );
    expect(editor.ring('Damage to you')).toBeNull();
    await editor.pick('Disarms and fading buffs');
    expect(editor.swatches()).toEqual([
      'The ## mark (Theme red)',
      'The line #c3a6ff The preset now has 178Take the fixKeep mine',
    ]);
    const cells = findAll(doc.body, (el) => hasClass(el, 'st-color-cell'));
    expect(cells.map((el) => hasClass(el, 'is-warn'))).toEqual([false, true]);
  });

  it('keeps your color at Save with the fix as its was', async () => {
    const editor = await mountEditor(['disarm_buff_fade'], [], {}, 'granted', null, FIXED);
    await editor.pick('Disarms and fading buffs');
    await editor.click('Keep mine');
    expect(editor.swatches()[1]).toBe('The line #c3a6ff Back to 178');
    expect(editor.ring('Disarms and fading buffs')).toBeNull();
    await editor.save();
    expect(sentCalls()[0][1]).toEqual({
      id: 'disarm_buff_fade',
      edits: { colors: { line: { value: '#c3a6ff', was: 'fg:178' } } },
      profile: undefined,
    });
  });

  it('takes the fix at Save as a row Rust drops', async () => {
    const editor = await mountEditor(['disarm_buff_fade'], [], {}, 'granted', null, FIXED);
    await editor.pick('Disarms and fading buffs');
    await editor.click('Take the fix');
    expect(editor.swatches()[1]).toBe('The line (178, #d7af00)');
    await editor.save();
    expect(sentCalls()[0][1]).toEqual({
      id: 'disarm_buff_fade',
      edits: { colors: { line: { value: 'fg:178', was: 'fg:178' } } },
      profile: undefined,
    });
  });

  it('folds a hex that is the preset color at Save', async () => {
    const editor = await mountEditor(['disarm_buff_fade'], [], {});
    await editor.pick('Disarms and fading buffs');
    await editor.typeColor('The line', '#D7AF00');
    await editor.save();
    expect(sentCalls()[0][1]).toEqual({
      id: 'disarm_buff_fade',
      edits: { colors: { line: { value: 'fg:178', was: 'fg:178' } } },
      profile: undefined,
    });
  });
});

const ORLA = {
  disarm_buff_fade: {
    ...LILAC.disarm_buff_fade,
    triggers: { 'buff.sanctuary': { enabled: { value: false, was: true } } },
  },
};

// Your changes, its links, and Reset to preset.
describe('Your changes and Reset to preset', () => {
  it('names each change, and its links open Triggers while the preset is on', async () => {
    const editor = await mountEditor(['disarm_buff_fade'], [], {}, 'granted', null, ORLA);
    await editor.pick('Disarms and fading buffs');
    expect(editor.rows()).toEqual([
      'Disarms and fading buffs',
      'Looks like',
      'Colors',
      'Adds',
      'Your changes',
    ]);
    expect(editor.value('Your changes')).toBe('The line color, buff.sanctuary');
    expect(editor.links()).toEqual(['Back to 178', '7 triggers', 'buff.sanctuary']);
    await editor.click('buff.sanctuary');
    await editor.click('7 triggers');
    expect(editor.opened).toEqual([
      { select: 'buff.sanctuary' },
      { filter: 'Disarms and fading buffs' },
    ]);
  });

  it('reads as plain text while the preset is off and says the edits are kept', async () => {
    const editor = await mountEditor(['none'], [], {}, 'granted', null, ORLA);
    await editor.pick('Disarms and fading buffs');
    expect(editor.links()).toEqual(['Back to 178']);
    expect(editor.value('Adds')).toBe('7 triggers');
    expect(editor.value('Your changes')).toBe(
      'Kept while the preset is off.The line color, buff.sanctuary',
    );
  });

  it('counts past two', async () => {
    const edits = {
      combat_incoming: {
        colors: {
          line: { value: '#999999', was: 'fg:244' },
          verb: { value: '#ff0000', was: 'fg:210' },
        },
        triggers: { 'combat.incoming': { enabled: { value: false, was: true } } },
      },
    };
    const editor = await mountEditor(['none'], [], {}, 'granted', null, edits);
    await editor.pick('Damage to you');
    expect(editor.value('Your changes')).toContain('2 colors and 1 trigger');
  });

  it('clears every edit with Reset to preset, off included, at Save', async () => {
    const editor = await mountEditor(['none'], [], {}, 'granted', null, ORLA);
    await editor.pick('Disarms and fading buffs');
    expect(editor.row('Disarms and fading buffs').edited).toBe(true);
    expect(editor.row('Herb labels').edited).toBeUndefined();
    await editor.press('Reset to preset');
    expect(editor.row('Disarms and fading buffs').edited).toBeUndefined();
    expect(editor.value('Your changes')).toBeUndefined();
    expect(editor.swatches()).toEqual(['The ## mark (Theme red)', 'The line (178, #d7af00)']);
    expect(editor.status()).toBe('Unsaved changes');

    await editor.save();
    expect(sentCalls()[0]).toEqual([
      'preset_edits_set',
      {
        id: 'disarm_buff_fade',
        edits: {
          colors: { line: { value: 'fg:178', was: 'fg:178' } },
          triggers: { 'buff.sanctuary': { enabled: { value: true, was: true } } },
        },
        profile: undefined,
      },
    ]);
  });

  it('takes a trigger out of the group you put it in, as its own Reset does', async () => {
    const edits = {
      disarm_buff_fade: {
        triggers: { 'buff.sanctuary': { group: { value: 'buffs', was: '' } } },
      },
    };
    const editor = await mountEditor(['disarm_buff_fade'], [], {}, 'granted', null, edits);
    bus.stored = [
      { name: 'buff.sanctuary', group: 'buffs' },
      { name: 'rest', group: 'mine' },
    ];
    await editor.pick('Disarms and fading buffs');
    await editor.press('Reset to preset');
    await editor.save();
    expect(sentCalls().map(([cmd]) => cmd)).toEqual([
      'preset_edits_set',
      'triggers_import',
      'presets_install',
    ]);
    expect(sentCalls()[0][1]).toEqual({
      id: 'disarm_buff_fade',
      edits: { triggers: { 'buff.sanctuary': { group: { value: '', was: '' } } } },
      profile: undefined,
    });
    expect(bus.stored).toEqual([{ name: 'buff.sanctuary' }, { name: 'rest', group: 'mine' }]);
  });

  it('clears the parts of an alert preset you changed', async () => {
    const editor = await mountEditor(['none'], [], { alert_tells: TELLS });
    await editor.pick('Tells you get');
    expect(editor.row('Tells you get').edited).toBe(true);
    await editor.press('Reset to preset');
    expect(editor.parts()).toEqual(['+Banner', 'Sound', 'Bounce']);
    await editor.save();
    expect(sentCalls()).toEqual([
      ['alert_presets_set', { id: 'alert_tells', alert: null, profile: undefined }],
    ]);

    await editor.pick('Your name');
    expect(findAll(doc.body, (el) => el.textContent === 'Reset to preset')).toEqual([]);
  });
});

const CHANGED_NOTE =
  'Your presets changed outside Settings while you edited them. Save keeps those changes and adds yours.';

// The card in the main window turns a preset on through
// presets_enabled_set while the Presets page is open.
describe('following the presets another window turns on', () => {
  it('loads the new list at once while the page is clean', async () => {
    const editor = await mountEditor(['none'], [], {});
    await editor.pick('Cures and heals');
    expect(editor.on()).toBe(false);
    bus.enabled = ['healing_basics'];
    await fire('vosh://presets-changed', { profile: null });
    expect(editor.on()).toBe(true);
    expect(editor.status()).toBe('');
    expect(editor.errors.filter(Boolean)).toEqual([]);
  });

  it('keeps your switch, says the list changed, and Save adds yours to it', async () => {
    const editor = await mountEditor(['none'], [], {});
    await editor.pick('Parries, dodges, and blocks');
    await editor.toggle();
    bus.enabled = ['healing_basics'];
    await fire('vosh://presets-changed', { profile: null });
    expect(editor.errors.at(-1)).toBe(CHANGED_NOTE);
    await editor.pick('Cures and heals');
    expect(editor.on()).toBe(false);

    await editor.save();
    const sent = bus.calls.filter(([cmd]) => cmd !== 'presets_remove');
    expect(sent.map(([cmd]) => cmd)).toEqual(['presets_enabled_set']);
    const { changes, triggers } = sent[0][1] as {
      changes: unknown[];
      triggers: { preset: string }[];
    };
    expect(changes).toEqual([{ id: 'defensive_combat', on: true }]);
    expect(new Set(triggers.map((t) => t.preset))).toEqual(
      new Set(['healing_basics', 'defensive_combat']),
    );
    expect(bus.enabled).toEqual(['healing_basics', 'defensive_combat']);
    expect(editor.on()).toBe(true);
    expect(editor.status()).not.toBe('Unsaved changes');
  });

  it('follows your preset edits too', async () => {
    const editor = await mountEditor(['none'], [], {});
    await editor.pick('Cures and heals');
    bus.enabled = ['healing_basics'];
    await fire('vosh://preset-edits-changed', { profile: null });
    expect(editor.on()).toBe(true);
  });
});

const ASK = [
  'primary',
  'Let Vosh post banners?',
  'Vosh posts banners only for the alerts you turn on. macOS asks you next.',
  'Not now',
  'Continue',
];
const OFF_NOTE =
  'Banners from Vosh are off in System Settings, so Banner shows nothing. Sound and Bounce still work.';

// Not now holds for the window, so its test runs last.
describe('asking before the first banner', () => {
  it('asks when you press Banner on, and Continue asks macOS and keeps its answer', async () => {
    const editor = await mountEditor([], [], {}, 'not_asked');
    await editor.pick('Low health');
    await editor.click('Banner');
    expect(editor.ask()).toBeUndefined();
    expect(editor.parts()).toEqual(['Banner', 'Sound', 'Bounce']);

    await editor.click('Banner');
    expect(editor.ask()).toEqual(ASK);
    bus.answer = 'granted';
    await editor.click('Continue');
    expect(bus.calls.map(([cmd]) => cmd)).toEqual(['alerts_ask_permission']);
    expect(editor.ask()).toBeUndefined();
    expect(editor.parts()).toEqual(['+Banner', 'Sound', 'Bounce']);
    expect(editor.banner()).toEqual({ warn: false, title: null });
    expect(editor.note()).toBeUndefined();
  });

  it('asks when you turn on Your name with Banner on, and rings Banner once macOS says no', async () => {
    const editor = await mountEditor([], [], {}, 'not_asked');
    await editor.pick('Your name');
    await editor.toggle();
    expect(editor.ask()).toEqual(ASK);
    bus.answer = 'denied';
    await editor.click('Continue');
    expect(editor.on()).toBe(true);
    expect(editor.banner()).toEqual({
      warn: true,
      title: 'Banners from Vosh are off in System Settings, so Banner shows nothing.',
    });
    expect(editor.note()).toBe(OFF_NOTE + 'Open notification settings');
  });

  it('rings Banner on every card while macOS turns banners off, and the button opens its settings', async () => {
    const editor = await mountEditor([], [], {}, 'denied');
    await editor.pick('Tells you get');
    expect(editor.banner().warn).toBe(true);
    expect(editor.note()).toBe(OFF_NOTE + 'Open notification settings');
    await editor.click('Open notification settings');
    expect(bus.calls).toEqual([['alerts_open_settings', undefined]]);

    await editor.click('Banner');
    await editor.click('Banner');
    await editor.toggle();
    expect(editor.ask()).toBeUndefined();
    await editor.pick('Connection');
    expect(editor.banner().warn).toBe(true);
  });

  it('asks nothing and rings nothing while banners are allowed or cannot post', async () => {
    for (const permission of ['granted', 'unavailable']) {
      const editor = await mountEditor([], [], {}, permission);
      await editor.pick('Your name');
      await editor.click('Banner');
      await editor.click('Banner');
      await editor.toggle();
      expect(editor.ask()).toBeUndefined();
      expect(editor.on()).toBe(true);
      expect(editor.banner()).toEqual({ warn: false, title: null });
      expect(editor.note()).toBeUndefined();
    }
    expect(bus.calls).toEqual([]);
  });

  it('leaves Banner on after Not now and asks no more in this window', async () => {
    const editor = await mountEditor([], [], {}, 'not_asked');
    await editor.pick('Being attacked');
    await editor.click('Banner');
    await editor.click('Banner');
    await editor.click('Not now');
    expect(editor.ask()).toBeUndefined();
    expect(editor.parts()).toEqual(['+Banner', 'Sound', 'Bounce']);

    await editor.click('Banner');
    await editor.click('Banner');
    await editor.pick('Your name');
    await editor.toggle();
    expect(editor.ask()).toBeUndefined();
    expect(editor.on()).toBe(true);
    expect(bus.calls).toEqual([]);
  });
});
