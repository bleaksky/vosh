import { act, createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { PluginRow } from '../../ipc/scripts';
import { FakeDocument, findAll, type FakeElement } from '../../test/fakeDom';
import { MISNAMED_NOTE, NO_PLUGINS, PluginList } from './PluginList';

// The Plugins section of Scripts, drawn as markup for what it shows and
// mounted for the switch, the menu and Install.

const calls = vi.hoisted(() => ({
  invoked: [] as { cmd: string; args: unknown }[],
  answer: (() => Promise.resolve([])) as (cmd: string) => Promise<unknown>,
}));

// The menu draws its rows in place, with no page to portal into, and a
// confirm draws its words and buttons with no focus trap, which needs
// more of the DOM than src/test/fakeDom.ts holds.
vi.mock('../../ui/MenuSurface', async (actual) => ({
  ...(await actual<typeof import('../../ui/MenuSurface')>()),
  MenuSurface: ({ label, children }: { label: string; children: ReactNode }) => (
    <menu aria-label={label}>{children}</menu>
  ),
}));
vi.mock('../../ui/ConfirmDialog', () => ({
  ConfirmDialog: (props: {
    title: string;
    body: string;
    confirmLabel: string;
    tone?: string;
    onConfirm: () => void;
    onCancel: () => void;
  }) => (
    <div role="dialog" aria-label={props.title} data-tone={props.tone ?? 'danger'}>
      <p>{props.body}</p>
      <button type="button" onClick={props.onCancel}>
        Cancel
      </button>
      <button type="button" onClick={props.onConfirm}>
        {props.confirmLabel}
      </button>
    </div>
  ),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: unknown) => {
    calls.invoked.push({ cmd, args });
    return calls.answer(cmd);
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const plugin = (patch: Partial<PluginRow> & Pick<PluginRow, 'name'>): PluginRow => ({
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

// Board 4's three plugins.
const BOARD: PluginRow[] = [
  plugin({
    name: 'vitals_alert',
    author: 'James Wright',
    description: 'Echo a warning to the terminal when HP drops below a configurable threshold.',
  }),
  plugin({
    name: 'wait_full',
    description: 'Stand up once your hit points are full.',
    stopped: 'time',
  }),
  plugin({
    name: 'weather_pane',
    version: '0.2.0',
    author: 'Tolliver',
    description: 'Show the weather, your position and your language in a pane of its own.',
    on: false,
  }),
];

const none = () => undefined;

function draw(plugins: PluginRow[] | null): string {
  return renderToStaticMarkup(
    <PluginList
      plugins={plugins}
      onPlugins={none}
      onError={none}
      onChanged={none}
      onNew={none}
      onOpen={none}
    />,
  );
}

/** Each row's name, description, meta and whether its switch is on. */
function rows(html: string) {
  return [...html.matchAll(/<div class="st-row st-plugin-row">(.*?)<\/button><\/div><\/div>/g)].map(
    ([row]) => ({
      name: /class="st-row-label">([^<]*)</.exec(row)?.[1],
      description: /class="st-row-desc">([^<]*)</.exec(row)?.[1],
      meta: /class="st-meta" data-tone="(\w+)">([^<]*)</.exec(row)?.slice(1),
      on: /<input[^>]*role="switch"[^>]*checked=""/.test(row),
      more: /aria-label="([^"]*)"/.exec(row.slice(row.lastIndexOf('<button')))?.[1],
    }),
  );
}

describe('the Plugins section', () => {
  it('heads the card with Install, New plugin and the book that opens Help on Lua', () => {
    const html = draw([]);
    expect(html).toContain('data-st-anchor="plugins"');
    expect(html).toMatch(/<h2[^>]*>Plugins<\/h2>/);
    expect(html).toMatch(/<button[^>]*>Install…<\/button><input type="file" accept=".zip"/);
    expect(html).toMatch(/class="btn has-icon">.*New plugin<\/button>/);
    expect(html).toContain('aria-label="Help on Lua scripts"');
  });

  it('says what a plugin is while you have none', () => {
    const html = draw([]);
    expect(html).toContain(
      `<div class="st-card"><p class="st-plugin-empty">${NO_PLUGINS}</p></div>`,
    );
    expect(rows(html)).toEqual([]);
  });

  it('shows neither the note nor a row until the list arrives', () => {
    expect(draw(null)).toContain('<div class="st-card"></div>');
  });

  it('draws a row for each plugin, and Stopped in the warn tone on a stopped one', () => {
    expect(rows(draw(BOARD))).toEqual([
      {
        name: 'vitals_alert',
        description: 'Echo a warning to the terminal when HP drops below a configurable threshold.',
        meta: undefined,
        on: true,
        more: 'vitals_alert options',
      },
      {
        name: 'wait_full',
        description: 'Stand up once your hit points are full.',
        meta: ['warn', 'Stopped'],
        on: true,
        more: 'wait_full options',
      },
      {
        name: 'weather_pane',
        description: 'Show the weather, your position and your language in a pane of its own.',
        meta: undefined,
        on: false,
        more: 'weather_pane options',
      },
    ]);
  });

  it('shows a plugin whose folder breaks the name rule, with no way to open it', () => {
    const html = draw([
      plugin({ name: 'weather-pane', description: 'Show the weather.', misnamed: true }),
      plugin({ name: 'old-pane', on: false, misnamed: true }),
    ]);
    const [running, off] = html.split('<div class="st-row st-plugin-row">').slice(1);
    // The note takes the description's place, and the row opens nothing.
    expect(running).toContain(
      `<div class="st-row-text"><span class="st-row-label">weather-pane</span><span class="st-row-desc">${MISNAMED_NOTE}</span></div>`,
    );
    expect(html).not.toContain('st-plugin-open');
    expect(html).not.toContain('Show the weather.');
    // A running one turns off, one that is off stays so, and neither
    // menu opens.
    expect(running).toMatch(/<input(?![^>]*disabled)[^>]*role="switch"[^>]*checked=""/);
    expect(off).toMatch(/<input[^>]*disabled=""[^>]*role="switch"/);
    for (const row of [running, off]) {
      expect(row).toMatch(/<button[^>]*aria-haspopup="menu"[^>]*disabled=""/);
    }
  });

  it('draws no description line for a plugin without one', () => {
    const html = draw([plugin({ name: 'wait_full' })]);
    expect(html).not.toContain('st-row-desc');
  });
});

describe('the On switch', () => {
  const doc = new FakeDocument();
  let createRoot: typeof import('react-dom/client').createRoot;

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
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  /** Mount the board's list, flip the switch of `name`, and say what
   *  the section handed back. */
  async function flip(name: string, on: boolean, plugins: PluginRow[] = BOARD) {
    calls.invoked.length = 0;
    const shown: PluginRow[][] = [];
    const errors: (string | null)[] = [];
    let reads = 0;
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    await act(async () => {
      root.render(
        createElement(PluginList, {
          plugins,
          onPlugins: (list) => void shown.push(list),
          onError: (e) => void errors.push(e),
          onChanged: () => void reads++,
          onNew: none,
          onOpen: none,
        }),
      );
    });
    const switches = findAll(container, (el) => el.getAttribute('role') === 'switch');
    const row = plugins.findIndex((p) => p.name === name);
    const input = switches[row] as FakeElement;
    const key = Object.keys(input).find((k) => k.startsWith('__reactProps$')) ?? '';
    const props = (input as unknown as Record<string, { onChange: (e: unknown) => void }>)[key];
    await act(async () => {
      props.onChange({ target: { checked: on } });
    });
    await act(async () => {
      root.unmount();
    });
    doc.body.removeChild(container);
    return { shown, errors, reads };
  }

  it('turns the plugin on or off for the profile you play', async () => {
    const after = BOARD.map((p) => (p.name === 'weather_pane' ? { ...p, on: true } : p));
    calls.answer = () => Promise.resolve(after);
    const { shown, errors, reads } = await flip('weather_pane', true);
    expect(calls.invoked).toEqual([
      { cmd: 'plugin_set_enabled', args: { name: 'weather_pane', on: true } },
    ]);
    // The switch moves at once, then the list the command hands back
    // settles it.
    expect(shown).toEqual([after, after]);
    expect(errors).toEqual([null]);
    expect(reads).toBe(0);
  });

  it('turns off a plugin whose folder breaks the name rule', async () => {
    calls.answer = () => Promise.resolve([]);
    const { errors } = await flip('weather-pane', false, [
      ...BOARD,
      plugin({ name: 'weather-pane', misnamed: true }),
    ]);
    expect(calls.invoked).toEqual([
      { cmd: 'plugin_set_enabled', args: { name: 'weather-pane', on: false } },
    ]);
    expect(errors).toEqual([null]);
  });

  it('shows the refusal and reads the list again when the change fails', async () => {
    calls.answer = () => Promise.reject('Vosh could not save your profile.');
    const { shown, errors, reads } = await flip('vitals_alert', false);
    expect(calls.invoked).toEqual([
      { cmd: 'plugin_set_enabled', args: { name: 'vitals_alert', on: false } },
    ]);
    expect(shown).toHaveLength(1);
    expect(shown[0][0].on).toBe(false);
    expect(errors).toEqual(['Vosh could not save your profile.']);
    expect(reads).toBe(1);
  });
});

describe('a press on the list', () => {
  const doc = new FakeDocument();
  let createRoot: typeof import('react-dom/client').createRoot;

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
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  /** Mount board 4's list, press the button `pick` finds, and say what
   *  the list asked for. */
  async function pressOn(pick: (el: FakeElement) => boolean) {
    calls.invoked.length = 0;
    const opened: string[] = [];
    let asked = 0;
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    await act(async () => {
      root.render(
        createElement(PluginList, {
          plugins: BOARD,
          onPlugins: none,
          onError: none,
          onChanged: none,
          onNew: () => void asked++,
          onOpen: (name) => void opened.push(name),
        }),
      );
    });
    const button = findAll(container, (el) => el.nodeName === 'BUTTON' && pick(el))[0];
    const key = Object.keys(button).find((k) => k.startsWith('__reactProps$')) ?? '';
    const props = (button as unknown as Record<string, { onClick: (e: unknown) => void }>)[key];
    await act(async () => {
      props.onClick({});
    });
    const switches = findAll(container, (el) => el.getAttribute('role') === 'switch').map((el) =>
      el.getAttribute('aria-label'),
    );
    await act(async () => {
      root.unmount();
    });
    doc.body.removeChild(container);
    return { opened, asked, switches };
  }

  it('opens the plugin from its row, and names each switch by its plugin', async () => {
    const { opened, asked, switches } = await pressOn(
      (el) =>
        el.getAttribute('class') === 'st-row-text st-plugin-open' &&
        el.textContent?.startsWith('wait_full') === true,
    );
    expect(opened).toEqual(['wait_full']);
    expect(asked).toBe(0);
    expect(calls.invoked).toEqual([]);
    expect(switches).toEqual(['vitals_alert', 'wait_full', 'weather_pane']);
  });

  it('asks for a new plugin from New plugin', async () => {
    const { opened, asked } = await pressOn((el) => el.textContent === 'New plugin');
    expect(asked).toBe(1);
    expect(opened).toEqual([]);
  });
});

describe('the more menu and Install', () => {
  const doc = new FakeDocument();
  let createRoot: typeof import('react-dom/client').createRoot;

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
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  /** Call a React handler on `el`. This DOM sends no events. */
  function handler<E>(el: FakeElement, name: string): (e: E) => void {
    const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
    return (el as unknown as Record<string, Record<string, (e: E) => void>>)[key][name];
  }

  /** Let every answer and file read settle. */
  const settle = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));

  /** Board 4's list, mounted, with what it handed back. */
  async function mount() {
    calls.invoked.length = 0;
    const shown: PluginRow[][] = [];
    const errors: (string | null)[] = [];
    let reads = 0;
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    await act(async () => {
      root.render(
        createElement(PluginList, {
          plugins: BOARD,
          onPlugins: (list) => void shown.push(list),
          onError: (e) => void errors.push(e),
          onChanged: () => void reads++,
          onNew: none,
          onOpen: none,
        }),
      );
    });
    const button = (text: string) =>
      findAll(container, (el) => el.nodeName === 'BUTTON' && el.textContent === text)[0];
    const press = async (el: FakeElement | undefined, e: unknown = {}) => {
      if (!el) throw new Error('nothing to press');
      await act(async () => handler(el, 'onClick')(e));
      await settle();
    };
    // The more button of `name` as a press hands it, placed where the
    // board draws vitals_alert's.
    const openMenu = (name: string) => {
      const more = findAll(container, (el) => el.getAttribute('aria-label') === `${name} options`);
      return press(more[0], {
        currentTarget: {
          getBoundingClientRect: () => ({ left: 812, right: 840, top: 92, bottom: 116 }),
          focus() {},
        },
      });
    };
    const pick = async (file: File) => {
      const input = findAll(container, (el) => el.getAttribute('type') === 'file')[0];
      await act(async () => handler(input, 'onChange')({ target: { files: [file], value: '' } }));
      await settle();
    };
    const text = (cls: string) =>
      findAll(container, (el) => el.getAttribute('class') === cls)[0]?.textContent;
    const dialog = () =>
      findAll(container, (el) => el.getAttribute('role') === 'dialog').map((el) => ({
        title: el.getAttribute('aria-label'),
        tone: el.getAttribute('data-tone'),
        body: el.childNodes[0].textContent,
      }))[0];
    const unmount = async () => {
      await act(async () => root.unmount());
      doc.body.removeChild(container);
    };
    return {
      container,
      shown,
      errors,
      reads: () => reads,
      button,
      press,
      openMenu,
      pick,
      text,
      dialog,
      unmount,
    };
  }

  it('opens the menu of a row and marks its more button open', async () => {
    const m = await mount();
    await m.openMenu('vitals_alert');
    const menu = findAll(m.container, (el) => el.nodeName === 'MENU')[0];
    expect(menu.getAttribute('aria-label')).toBe('vitals_alert options');
    const expanded = findAll(m.container, (el) => el.getAttribute('aria-haspopup') === 'menu').map(
      (el) => el.getAttribute('aria-expanded'),
    );
    expect(expanded).toEqual(['true', 'false', 'false']);
    await m.unmount();
  });

  it('reloads the plugin and shows the list Reload hands back', async () => {
    const after = BOARD.map((p) => ({ ...p, stopped: null }));
    calls.answer = () => Promise.resolve(after);
    const m = await mount();
    await m.openMenu('wait_full');
    await m.press(m.button('Reload'));
    expect(calls.invoked).toEqual([{ cmd: 'plugin_reload', args: { name: 'wait_full' } }]);
    expect(m.shown).toEqual([after]);
    expect(m.errors).toEqual([null]);
    expect(findAll(m.container, (el) => el.nodeName === 'MENU')).toEqual([]);
    await m.unmount();
  });

  it('shows the folder in the file manager', async () => {
    calls.answer = () => Promise.resolve(null);
    const m = await mount();
    await m.openMenu('vitals_alert');
    await m.press(m.button('Show the folder'));
    expect(calls.invoked).toEqual([{ cmd: 'plugin_reveal', args: { name: 'vitals_alert' } }]);
    expect(m.errors).toEqual([null]);
    await m.unmount();
  });

  it('says under the list where an export went', async () => {
    calls.answer = () => Promise.resolve('vitals_alert.zip');
    const m = await mount();
    expect(m.text('st-plugin-status')).toBe('');
    await m.openMenu('vitals_alert');
    await m.press(m.button('Export to Downloads'));
    expect(calls.invoked).toEqual([{ cmd: 'plugin_export', args: { name: 'vitals_alert' } }]);
    expect(m.text('st-plugin-status')).toBe(
      'Vosh saved vitals_alert.zip in your Downloads folder.',
    );
    await m.unmount();
  });

  it('asks before Remove, then removes the plugin from every profile', async () => {
    const after = BOARD.filter((p) => p.name !== 'weather_pane');
    calls.answer = () => Promise.resolve(after);
    const m = await mount();
    await m.openMenu('weather_pane');
    await m.press(m.button('Remove…'));
    expect(calls.invoked).toEqual([]);
    expect(m.dialog()).toEqual({
      title: 'Remove weather_pane?',
      tone: 'danger',
      body: 'Vosh deletes the weather_pane folder and turns the plugin off in every profile. You cannot undo this.',
    });
    await m.press(m.button('Remove'));
    expect(calls.invoked).toEqual([{ cmd: 'plugin_remove', args: { name: 'weather_pane' } }]);
    expect(m.shown).toEqual([after]);
    expect(m.dialog()).toBeUndefined();
    await m.unmount();
  });

  it('asks once about a .zip you pick, then installs it', async () => {
    const check = { name: 'weather_pane', version: '0.2.0', author: 'Tolliver', existing: null };
    calls.answer = (cmd) => Promise.resolve(cmd === 'plugin_install_check' ? check : BOARD);
    const m = await mount();
    await m.pick(new File(['PK'], 'weather_pane.zip'));
    const sent = { fileName: 'weather_pane.zip', bytes: [80, 75] };
    expect(calls.invoked).toEqual([{ cmd: 'plugin_install_check', args: sent }]);
    expect(m.dialog()).toMatchObject({ title: 'Install weather_pane?', tone: 'primary' });
    await m.press(m.button('Install'));
    expect(calls.invoked.slice(1)).toEqual([{ cmd: 'plugin_install', args: sent }]);
    expect(m.shown).toEqual([BOARD]);
    expect(m.dialog()).toBeUndefined();
    await m.unmount();
  });

  it('asks once about a .zip you drop anywhere on the list page', async () => {
    // The fake document keeps no listeners, so this one records them.
    type Listener = (e: unknown) => void;
    const listeners = new Map<string, Listener>();
    const on = doc as unknown as {
      addEventListener: (type: string, fn: Listener) => void;
      removeEventListener: (type: string, fn: Listener) => void;
    };
    on.addEventListener = (type, fn) => void listeners.set(type, fn);
    on.removeEventListener = (type, fn) => {
      if (listeners.get(type) === fn) listeners.delete(type);
    };
    const check = { name: 'weather_pane', version: '0.2.0', author: 'Tolliver', existing: null };
    calls.answer = () => Promise.resolve(check);
    const m = await mount();
    expect([...listeners.keys()].sort()).toEqual(['dragover', 'drop']);

    // A drag that carries text passes by, and one with files takes the drop.
    const event = (types: string[], items: unknown[] = []) => ({
      dataTransfer: { types, items, dropEffect: 'none' },
      preventDefault: vi.fn(),
    });
    const text = event(['text/plain']);
    listeners.get('dragover')?.(text);
    expect(text.preventDefault).not.toHaveBeenCalled();
    const over = event(['Files']);
    listeners.get('dragover')?.(over);
    expect(over.preventDefault).toHaveBeenCalled();
    expect(over.dataTransfer.dropEffect).toBe('copy');

    const zip = new File(['PK'], 'weather_pane.zip');
    const entry = {
      isDirectory: false,
      name: zip.name,
      file: (resolve: (file: File) => void) => resolve(zip),
    };
    const drop = event(['Files'], [{ webkitGetAsEntry: () => entry }]);
    await act(async () => listeners.get('drop')?.(drop));
    await act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));
    expect(drop.preventDefault).toHaveBeenCalled();
    expect(calls.invoked).toEqual([
      {
        cmd: 'plugin_install_check',
        args: { fileName: 'weather_pane.zip', bytes: [80, 75] },
      },
    ]);
    expect(m.dialog()).toMatchObject({ title: 'Install weather_pane?', tone: 'primary' });

    await m.unmount();
    expect([...listeners.keys()]).toEqual([]);
    delete (doc as unknown as Record<string, unknown>).addEventListener;
    delete (doc as unknown as Record<string, unknown>).removeEventListener;
  });

  it('shows why Vosh refuses a plugin in the error line, and asks nothing', async () => {
    calls.answer = () => Promise.reject('Vosh found no manifest.toml in weather_pane.zip.');
    const m = await mount();
    await m.pick(new File(['PK'], 'weather_pane.zip'));
    expect(m.errors).toEqual(['Vosh found no manifest.toml in weather_pane.zip.']);
    expect(m.dialog()).toBeUndefined();
    await m.unmount();
  });

  it('shows a refusal at install in the error line and reads the list again', async () => {
    const check = { name: 'weather_pane', version: '0.2.0', author: 'Tolliver', existing: null };
    calls.answer = (cmd) =>
      cmd === 'plugin_install_check'
        ? Promise.resolve(check)
        : Promise.reject('Vosh could not install weather_pane.zip.');
    const m = await mount();
    await m.pick(new File(['PK'], 'weather_pane.zip'));
    await m.press(m.button('Install'));
    expect(m.errors).toEqual([null, 'Vosh could not install weather_pane.zip.']);
    expect(m.reads()).toBe(1);
    expect(m.dialog()).toBeUndefined();
    await m.unmount();
  });
});
