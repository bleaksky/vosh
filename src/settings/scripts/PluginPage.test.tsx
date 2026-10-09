import { act, createElement, type ReactNode } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LuaLine, PluginFolder, PluginRow } from '../../ipc/scripts';
import type { SettingsTarget } from '../../lib/settingsNav';
import { FakeDocument, findAll, type FakeElement } from '../../test/fakeDom';
import type { LeaveGuard } from '../pageTypes';
import type { ScriptsPage as ScriptsPageType } from './ScriptsPage';

// Scripts with a plugin's page open, mounted in the stand in for the
// DOM in src/test/fakeDom.ts. CodeMirror needs a real DOM, so the editor
// is a stand in that keeps its props, and so is the confirm card, which
// draws its title and fields and keeps its props for the buttons.

const calls = vi.hoisted(() => ({
  invoked: [] as { cmd: string; args: Record<string, unknown> | undefined }[],
  answers: {} as Record<string, (args: Record<string, unknown> | undefined) => unknown>,
}));
const seen = vi.hoisted(() => ({
  editor: null as Record<string, unknown> | null,
  dialog: null as Record<string, unknown> | null,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: Record<string, unknown>) => {
    calls.invoked.push({ cmd, args });
    const answer = calls.answers[cmd];
    try {
      return Promise.resolve(answer ? answer(args) : null);
    } catch (e) {
      return Promise.reject(e);
    }
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
vi.mock('../../ui/CodeEditor', () => ({
  CodeEditor: (props: Record<string, unknown>) => {
    seen.editor = props;
    return null;
  },
}));
vi.mock('../../ui/ConfirmDialog', async () => {
  const { createElement: h } = await import('react');
  return {
    ConfirmDialog: (props: Record<string, unknown> & { title: string; children?: ReactNode }) => {
      seen.dialog = props;
      return h('div', { role: 'dialog' }, h('h2', null, props.title), props.children);
    },
  };
});

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let ScriptsPage: typeof ScriptsPageType;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
  });
  vi.stubGlobal('navigator', { userAgent: 'node' });
  ({ createRoot } = await import('react-dom/client'));
  ({ ScriptsPage } = await import('./ScriptsPage'));
});

afterAll(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const at = (h: number, m: number, s: number) => new Date(2026, 9, 4, h, m, s).getTime();

const VITALS_ALERT_CODE = [
  '-- vitals_alert',
  'local THRESHOLD = 0.5',
  'mud.on_gmcp("Char.Vitals", function(data)',
  '  local hp = tonumber(data.hp)',
  'end)',
  '',
].join('\n');

const WAIT_FULL_CODE = [
  '-- wait_full',
  '-- Stand up once your hit points are full.',
  '',
  'mud.on_gmcp("Char.Vitals", function(data)',
  '  while data.hp < data.maxhp do',
  '    -- data never changes inside this loop, so it never ends',
  '  end',
  '  mud.send("stand")',
  'end)',
  '',
].join('\n');

const folder = (name: string, code: string, patch: Partial<PluginFolder> = {}): PluginFolder => ({
  manifest: {
    name,
    version: '0.1.0',
    description: '',
    author: name === 'vitals_alert' ? 'James Wright' : '',
    entry: 'main.lua',
  },
  code,
  files: ['main.lua'],
  folder: `plugins/${name}`,
  ...patch,
});

const row = (name: string, patch: Partial<PluginRow> = {}): PluginRow => ({
  name,
  version: '0.1.0',
  author: '',
  description: '',
  entry: 'main.lua',
  on: true,
  stopped: null,
  loaded_ms: null,
  misnamed: false,
  ...patch,
});

const STOP = 'Vosh stopped wait_full at main.lua line 5 after 100 ms.';

// The Output ring with the lines of vitals_alert and the stop of
// wait_full.
const RING: LuaLine[] = [
  {
    ts_ms: at(21, 14, 3),
    owner: 'plugin:vitals_alert',
    kind: 'error',
    text: "vitals_alert/main.lua:22: attempt to concatenate a nil value (field 'hp_pct')",
    at: { source: 'vitals_alert/main.lua', line: 22 },
  },
  {
    ts_ms: at(21, 14, 31),
    owner: 'plugin:vitals_alert',
    kind: 'note',
    text: 'Vosh reloaded vitals_alert.',
  },
  {
    ts_ms: at(21, 20, 11),
    owner: 'plugin:wait_full',
    kind: 'error',
    text: STOP,
    at: { source: 'wait_full/main.lua', line: 5 },
  },
  {
    ts_ms: at(21, 20, 11),
    owner: 'plugin:wait_full',
    kind: 'note',
    text: 'wait_full stays off until you save it under Scripts in Settings or restart Vosh.',
  },
  { ts_ms: at(21, 21, 0), owner: '#lua', kind: 'print', text: 'a bank representative' },
];

let LIST: PluginRow[] = [];

beforeEach(() => {
  calls.invoked.length = 0;
  seen.editor = null;
  seen.dialog = null;
  // vitals_alert loaded again at 21:14:31, after its error at line 22.
  LIST = [
    row('vitals_alert', { loaded_ms: at(21, 14, 31) }),
    row('wait_full', { stopped: 'time', loaded_ms: at(21, 19, 0) }),
  ];
  calls.answers = {
    plugins_list: () => LIST,
    lua_output_get: () => RING,
    plugin_read: (args) =>
      args?.name === 'wait_full'
        ? folder('wait_full', WAIT_FULL_CODE)
        : folder('vitals_alert', VITALS_ALERT_CODE),
    plugin_save: () => LIST,
    plugin_create: (args) => [...LIST, row(String(args?.name))],
  };
});

/** Call a React handler on `el`. This DOM sends no events. */
function handler<E>(el: FakeElement, name: string): (e: E) => void {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
  return (el as unknown as Record<string, Record<string, (e: E) => void>>)[key][name];
}

const button = (root: FakeElement, label: string) =>
  findAll(root, (el) => el.nodeName === 'BUTTON' && el.textContent === label)[0];

interface Mounted {
  container: FakeElement;
  went: SettingsTarget[];
  errors: (string | null)[];
  guard: () => LeaveGuard | null;
  press: (label: string) => Promise<void>;
  /** Call `name` on the element `pick` finds with `event`. */
  fire: (pick: (el: FakeElement) => boolean, name: string, event: unknown) => Promise<void>;
  /** Run `fn` inside act and let the answers it waits on land. */
  step: (fn: () => void) => Promise<void>;
  /** Draw the page again at another target. */
  show: (target: SettingsTarget) => Promise<void>;
  unmount: () => Promise<void>;
}

async function mount(target: SettingsTarget): Promise<Mounted> {
  const went: SettingsTarget[] = [];
  const errors: (string | null)[] = [];
  let guard: LeaveGuard | null = null;
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const step = async (fn: () => void) => {
    await act(async () => {
      fn();
      for (let i = 0; i < 5; i++) await Promise.resolve();
    });
  };
  const show = (to: SettingsTarget) =>
    step(() =>
      root.render(
        createElement(ScriptsPage, {
          target: to,
          navSeq: 0,
          config: null,
          setConfig: () => undefined,
          onError: (e) => void errors.push(e),
          pathB: false,
          navigate: (t) => void went.push(t),
          setLeaveGuard: (g) => {
            guard = g;
          },
        }),
      ),
    );
  await show(target);
  return {
    container,
    went,
    errors,
    guard: () => guard,
    press: (label) => step(() => handler<unknown>(button(container, label), 'onClick')({})),
    fire: (pick, name, event) =>
      step(() => handler<unknown>(findAll(container, pick)[0], name)(event)),
    step,
    show,
    unmount: async () => {
      await act(async () => {
        root.unmount();
      });
      doc.body.removeChild(container);
    },
  };
}

const editor = () => seen.editor as Record<string, unknown> & { onChange: (v: string) => void };

const status = (root: FakeElement) =>
  findAll(root, (el) => el.getAttribute('class') === 'st-savebar-status')[0].textContent;

const off = (root: FakeElement, label: string) => button(root, label).hasAttribute('disabled');

describe('a plugin page', () => {
  it('opens on the file it runs first, with the switch at the right', async () => {
    const m = await mount({ group: 'scripts', section: 'vitals_alert' });
    expect(calls.invoked).toContainEqual({
      cmd: 'plugin_read',
      args: { name: 'vitals_alert', file: undefined },
    });
    const pressed = findAll(m.container, (el) => el.getAttribute('aria-pressed') === 'true');
    expect(pressed.map((el) => el.textContent)).toEqual(['main.lua']);
    expect(editor().value).toBe(VITALS_ALERT_CODE);
    expect(editor().page).toBe(true);
    expect(editor().className).toBe('st-code st-plugin-code');
    const toggle = findAll(m.container, (el) => el.getAttribute('role') === 'switch')[0];
    expect(toggle.getAttribute('id')).toBe(
      findAll(
        m.container,
        (el) => el.nodeName === 'LABEL' && el.textContent === 'On for this profile',
      )[0].getAttribute('for'),
    );
    expect(off(m.container, 'Discard')).toBe(true);
    expect(off(m.container, 'Save and reload')).toBe(true);
    expect(status(m.container)).toBe('');
    await m.unmount();
  });

  it('holds a draft until Discard puts the file back', async () => {
    const m = await mount({ group: 'scripts', section: 'vitals_alert' });
    await m.step(() => editor().onChange(`${VITALS_ALERT_CODE}-- more\n`));
    expect(off(m.container, 'Discard')).toBe(false);
    expect(off(m.container, 'Save and reload')).toBe(false);
    expect(status(m.container)).toBe('Unsaved changes');
    await m.press('Discard');
    expect(editor().value).toBe(VITALS_ALERT_CODE);
    expect(off(m.container, 'Save and reload')).toBe(true);
    expect(status(m.container)).toBe('');
    await m.unmount();
  });

  it('saves the manifest and the code, then says when it reloaded', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date(2026, 9, 4, 21, 14, 31));
    const m = await mount({ group: 'scripts', section: 'vitals_alert' });
    const code = VITALS_ALERT_CODE.replace('0.5', '0.4');
    await m.step(() => editor().onChange(code));
    await m.press('Manifest');
    await m.fire((el) => el.nodeName === 'INPUT' && el.value === '0.1.0', 'onChange', {
      target: { value: '0.2.0' },
    });
    calls.invoked.length = 0;
    await m.press('Save and reload');
    expect(calls.invoked[0]).toEqual({
      cmd: 'plugin_save',
      args: {
        name: 'vitals_alert',
        manifest: {
          name: 'vitals_alert',
          version: '0.2.0',
          description: '',
          author: 'James Wright',
          entry: 'main.lua',
        },
        code,
      },
    });
    expect(status(m.container)).toBe('Reloaded at 21:14');
    expect(off(m.container, 'Save and reload')).toBe(true);
    // The time holds when you leave the page and come back.
    await m.show({ group: 'scripts' });
    await m.show({ group: 'scripts', section: 'vitals_alert' });
    expect(status(m.container)).toBe('Reloaded at 21:14');
    vi.useRealTimers();
    await m.unmount();
  });

  it('says Saved for a plugin the profile keeps off', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date(2026, 9, 4, 9, 5, 0));
    LIST = [row('vitals_alert', { on: false }), row('wait_full')];
    const m = await mount({ group: 'scripts', section: 'vitals_alert' });
    await m.step(() => editor().onChange('-- off\n'));
    await m.press('Save and reload');
    expect(status(m.container)).toBe('Saved at 09:05');
    vi.useRealTimers();
    await m.unmount();
  });

  it('asks before the crumb or another group drops a draft', async () => {
    const m = await mount({ group: 'scripts', section: 'vitals_alert' });
    const guard = m.guard();
    expect(guard).not.toBeNull();
    // A clean page lets the move go.
    expect(guard?.(() => undefined)).toBe(false);
    await m.step(() => editor().onChange('-- edited\n'));
    let went = 0;
    let held = false;
    await m.step(() => {
      held = m.guard()?.(() => void went++) ?? false;
    });
    expect(held).toBe(true);
    expect(seen.dialog?.title).toBe('Discard changes to vitals_alert?');
    expect(seen.dialog?.body).toBe('Vosh keeps what you saved last.');
    expect(seen.dialog?.confirmLabel).toBe('Discard');
    await m.step(() => (seen.dialog?.onConfirm as () => void)());
    expect(went).toBe(1);
    await m.unmount();
  });
});

describe('Runs first', () => {
  const DRAW = '-- draw\nreturn {}\n';
  const FILES = ['lib/draw.lua', 'main.lua'];

  beforeEach(() => {
    calls.answers.plugin_read = (args) =>
      folder('vitals_alert', args?.file === 'lib/draw.lua' ? DRAW : VITALS_ALERT_CODE, {
        files: FILES,
      });
  });

  /** Pick `file` under Runs first on the Manifest tab. */
  const pick = (m: Mounted, file: string) =>
    m.fire((el) => el.nodeName === 'SELECT', 'onChange', { target: { value: file } });

  it('opens the file you pick, and Save writes it as the file the plugin runs first', async () => {
    const m = await mount({ group: 'scripts', section: 'vitals_alert' });
    await m.press('Manifest');
    calls.invoked.length = 0;
    await pick(m, 'lib/draw.lua');
    expect(calls.invoked).toEqual([
      { cmd: 'plugin_read', args: { name: 'vitals_alert', file: 'lib/draw.lua' } },
    ]);
    expect(seen.dialog).toBeNull();
    // The tab takes the file's name, and the editor its code.
    await m.press('lib/draw.lua');
    expect(editor().value).toBe(DRAW);
    const code = `${DRAW}-- more\n`;
    await m.step(() => editor().onChange(code));
    calls.invoked.length = 0;
    await m.press('Save and reload');
    expect(calls.invoked[0]).toEqual({
      cmd: 'plugin_save',
      args: {
        name: 'vitals_alert',
        manifest: {
          name: 'vitals_alert',
          version: '0.1.0',
          description: '',
          author: 'James Wright',
          entry: 'lib/draw.lua',
        },
        code,
      },
    });
    await m.unmount();
  });

  it('asks before a pick drops an edit to the file in the editor', async () => {
    const m = await mount({ group: 'scripts', section: 'vitals_alert' });
    const edited = `${VITALS_ALERT_CODE}-- more\n`;
    await m.step(() => editor().onChange(edited));
    await m.press('Manifest');
    calls.invoked.length = 0;
    await pick(m, 'lib/draw.lua');
    // Nothing is read until you agree, so Save never writes the old
    // file's code over the new one.
    expect(calls.invoked).toEqual([]);
    expect(seen.dialog?.title).toBe('Discard changes to main.lua?');
    await m.step(() => (seen.dialog?.onCancel as () => void)());
    await m.press('main.lua');
    expect(editor().value).toBe(edited);
    await m.press('Manifest');
    await pick(m, 'lib/draw.lua');
    await m.step(() => (seen.dialog?.onConfirm as () => void)());
    expect(calls.invoked).toEqual([
      { cmd: 'plugin_read', args: { name: 'vitals_alert', file: 'lib/draw.lua' } },
    ]);
    await m.press('lib/draw.lua');
    expect(editor().value).toBe(DRAW);
    await m.unmount();
  });

  it('goes back to the saved file without a read or a question', async () => {
    const m = await mount({ group: 'scripts', section: 'vitals_alert' });
    await m.press('Manifest');
    await pick(m, 'lib/draw.lua');
    seen.dialog = null;
    calls.invoked.length = 0;
    await pick(m, 'main.lua');
    expect(calls.invoked).toEqual([]);
    expect(seen.dialog).toBeNull();
    await m.press('main.lua');
    expect(editor().value).toBe(VITALS_ALERT_CODE);
    expect(off(m.container, 'Save and reload')).toBe(true);
    await m.unmount();
  });
});

describe('a stopped plugin', () => {
  it('says why over a shorter editor and marks the line the stop names', async () => {
    const m = await mount({ group: 'scripts', section: 'wait_full' });
    const note = findAll(m.container, (el) => el.getAttribute('class') === 'st-card-note is-warn');
    expect(note[0].textContent).toBe(
      'Vosh stopped wait_full because one call ran past 100 ms. It stays off until you save it or restart Vosh.',
    );
    expect(editor().className).toBe('st-code st-plugin-code is-short');
    expect(editor().marks).toEqual([{ line: 5, message: STOP }]);
    await m.unmount();
  });

  it('marks nothing and draws no note once it runs again', async () => {
    const m = await mount({ group: 'scripts', section: 'vitals_alert' });
    expect(
      findAll(m.container, (el) => el.getAttribute('class') === 'st-card-note is-warn'),
    ).toEqual([]);
    // The error at line 22 came before the reload that fixed it.
    expect(editor().marks).toEqual([]);
    await m.unmount();
  });

  it('marks an error from before a load the page saw no note for', async () => {
    // The switch or a profile switch loads a plugin without a note, and
    // the row still says when.
    LIST = [row('vitals_alert', { loaded_ms: at(21, 14, 2) })];
    const m = await mount({ group: 'scripts', section: 'vitals_alert' });
    expect(editor().marks).toEqual([{ line: 22, message: RING[0].text }]);
    LIST = [row('vitals_alert', { loaded_ms: at(21, 14, 4) })];
    await m.show({ group: 'scripts' });
    await m.show({ group: 'scripts', section: 'vitals_alert' });
    expect(editor().marks).toEqual([]);
    await m.unmount();
  });
});

describe('Output on a plugin page', () => {
  it('shows the lines of its plugin alone', async () => {
    const m = await mount({ group: 'scripts', section: 'wait_full' });
    const lines = findAll(m.container, (el) =>
      (el.getAttribute('class') ?? '').split(' ').includes('st-lua-line'),
    ).map((el) => el.textContent);
    expect(lines).toEqual([
      `21:20:11[lua] ${STOP}`,
      '21:20:11[lua] wait_full stays off until you save it under Scripts in Settings or restart Vosh.',
    ]);
    expect(findAll(m.container, (el) => el.textContent === 'Output')).not.toEqual([]);
    await m.unmount();
  });

  it('runs the console inside the plugin and clears its lines alone', async () => {
    const m = await mount({ group: 'scripts', section: 'vitals_alert' });
    const field = (el: FakeElement) => el.getAttribute('aria-label') === 'Run Lua in vitals_alert';
    expect(findAll(m.container, field)[0].getAttribute('placeholder')).toBe(
      'Run Lua in vitals_alert',
    );
    await m.fire(field, 'onChange', { target: { value: 'print(mud.var("target"))' } });
    calls.invoked.length = 0;
    await m.fire(field, 'onKeyDown', { key: 'Enter', preventDefault() {} });
    expect(calls.invoked).toEqual([
      { cmd: 'lua_run', args: { code: 'print(mud.var("target"))', plugin: 'vitals_alert' } },
    ]);
    calls.invoked.length = 0;
    await m.press('Clear');
    expect(calls.invoked).toEqual([
      { cmd: 'lua_output_clear', args: { owner: 'plugin:vitals_alert' } },
    ]);
    // The other plugin keeps its lines.
    await m.show({ group: 'scripts', section: 'wait_full' });
    expect(
      findAll(m.container, (el) =>
        (el.getAttribute('class') ?? '').split(' ').includes('st-lua-line'),
      ),
    ).toHaveLength(2);
    await m.unmount();
  });
});

describe('New plugin', () => {
  const name = (el: FakeElement) =>
    el.nodeName === 'INPUT' && el.getAttribute('aria-describedby') !== null;
  const hint = (root: FakeElement) =>
    findAll(root, (el) => el.getAttribute('class') === 'ov-hint')[0].textContent;

  it('keeps Create off for a name that breaks the rule or that you have', async () => {
    const m = await mount({ group: 'scripts' });
    await m.press('New plugin');
    expect(seen.dialog?.title).toBe('New plugin');
    expect(seen.dialog?.tone).toBe('primary');
    expect(seen.dialog?.confirmDisabled).toBe(true);
    expect(hint(m.container)).toBe('Letters, digits, and underscores.');
    await m.fire(name, 'onChange', { target: { value: 'wait full' } });
    expect(seen.dialog?.confirmDisabled).toBe(true);
    await m.fire(name, 'onChange', { target: { value: 'Wait_Full' } });
    expect(seen.dialog?.confirmDisabled).toBe(true);
    expect(hint(m.container)).toBe('You already have a plugin named wait_full.');
    await m.fire(name, 'onChange', { target: { value: 'weather_pane' } });
    expect(seen.dialog?.confirmDisabled).toBe(false);
    expect(hint(m.container)).toBe('Letters, digits, and underscores.');
    await m.unmount();
  });

  it('makes the plugin with Create and opens its page', async () => {
    const m = await mount({ group: 'scripts' });
    await m.press('New plugin');
    await m.fire(name, 'onChange', { target: { value: 'weather_pane' } });
    calls.invoked.length = 0;
    await m.step(() => (seen.dialog?.onConfirm as () => void)());
    expect(calls.invoked).toEqual([{ cmd: 'plugin_create', args: { name: 'weather_pane' } }]);
    expect(m.went).toEqual([{ group: 'scripts', section: 'weather_pane' }]);
    expect(findAll(m.container, (el) => el.getAttribute('role') === 'dialog')).toEqual([]);
    await m.unmount();
  });

  it('makes it with Enter in the field too, and stays open when Vosh refuses', async () => {
    calls.answers.plugin_create = () => {
      throw 'Vosh could not make the folder for weather_pane.';
    };
    const m = await mount({ group: 'scripts' });
    await m.press('New plugin');
    await m.fire(name, 'onChange', { target: { value: 'weather_pane' } });
    await m.fire(name, 'onKeyDown', { key: 'Enter', preventDefault() {} });
    expect(m.errors).toContain('Vosh could not make the folder for weather_pane.');
    expect(m.went).toEqual([]);
    expect(findAll(m.container, (el) => el.getAttribute('role') === 'dialog')).toHaveLength(1);
    await m.unmount();
  });
});
