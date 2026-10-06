import { act, createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { PluginRow } from '../../ipc/scripts';
import { FakeDocument, findAll, type FakeElement } from '../../test/fakeDom';
import { NO_PLUGINS, PluginList } from './PluginList';

// The Plugins section of Scripts, drawn as markup for what it shows and
// mounted for the switch.

const calls = vi.hoisted(() => ({
  invoked: [] as { cmd: string; args: unknown }[],
  answer: (() => Promise.resolve([])) as (cmd: string) => Promise<unknown>,
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
    <PluginList plugins={plugins} onPlugins={none} onError={none} onChanged={none} />,
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
  it('heads the card with New plugin and the book that opens Help on Lua', () => {
    const html = draw([]);
    expect(html).toContain('data-st-anchor="plugins"');
    expect(html).toMatch(/<h2[^>]*>Plugins<\/h2>/);
    expect(html).toMatch(
      /class="st-button st-button-secondary st-button-iconed">.*New plugin<\/button>/,
    );
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
  async function flip(name: string, on: boolean) {
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
        }),
      );
    });
    const switches = findAll(container, (el) => el.getAttribute('role') === 'switch');
    const row = BOARD.findIndex((p) => p.name === name);
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
