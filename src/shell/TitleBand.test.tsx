import { act, createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import tauriConf from '../../src-tauri/tauri.conf.json';
import APP_SHORTCUTS from '../lib/appShortcuts.json';
import { shortcutLabel } from '../lib/shortcuts';
import type { LuaPane } from '../ipc/panes';
import type { PluginRow } from '../ipc/scripts';
import { PANEL_WIDTH_MIN, panelWidthFloor, type PaneSplit } from '../panel/paneLayout';
import type { Connection } from '../stores/session/useConnection';
import frameCss from '../styles/frame.css?raw';
import { FakeDocument, FakeElement, findAll } from '../test/fakeDom';
import { ADD_PANE_MENU_EVENT } from '../lib/appMenu';
import { TitleBand } from './TitleBand';

// The title band's buttons at the right end: Add a pane while the panel
// shows, Search commands, the panel toggle, and Settings, then the
// window controls on Windows and Linux. Windows and Linux have no menu
// bar, so the gear is the button that shows you where Settings lives.

// The session button totals what waits behind, from stores that listen.
vi.mock('@tauri-apps/api/event', () => ({
  listen: async () => () => undefined,
  emit: async () => undefined,
}));

// A menu stands in for the band's own, which places itself by the
// layout the stand in DOM below has none of.
vi.mock('./ShellMenu', async (actual) => ({
  ...(await actual<typeof import('./ShellMenu')>()),
  ShellMenu: ({ label, children }: { label: string; children: ReactNode }) => (
    <div role="menu" aria-label={label}>
      {children}
    </div>
  ),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    setTitle: () => Promise.resolve(),
    isMaximized: () => Promise.resolve(false),
    onResized: () => Promise.resolve(() => undefined),
  }),
}));

// The staff queues store behind Add a pane reaches the Tauri bridge
// when it starts.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
// The Lua panes the session in front holds and its plugin rows, none
// unless a test sets them.
const lua = vi.hoisted(() => ({
  panes: new Map<string, unknown>(),
  rows: null as unknown[] | null,
  /** How many draws read the Lua pane store. */
  reads: 0,
}));
vi.mock('../stores/session/luaPanesStore', () => ({
  useLuaPanes: () => {
    lua.reads += 1;
    return lua.panes;
  },
  getLuaPanes: () => lua.panes,
}));
vi.mock('../stores/session/pluginRowsStore', () => ({
  usePluginRows: () => lua.rows,
  getPluginRows: () => lua.rows,
}));
// Add a pane draws its rows in place, with no page to portal into.
vi.mock('./ShellMenu', async (actual) => ({
  ...(await actual<typeof import('./ShellMenu')>()),
  ShellMenu: ({ label, children }: { label: string; children: ReactNode }) => (
    <div role="menu" aria-label={label}>
      {children}
    </div>
  ),
}));

const connection: Connection = {
  status: { kind: 'idle' },
  live: false,
  redialing: false,
  target: { host: 'play.theforsakenlands.com', port: 1848, tls: false },
  world: 'Aabahran',
  character: null,
  connect: () => Promise.resolve(),
  connectNew: () => Promise.resolve(),
  disconnect: () => Promise.resolve(),
  saveTarget: () => undefined,
};

const props = {
  connection,
  onTogglePanel: () => undefined,
  onTogglePalette: () => undefined,
  onOpenSettings: () => undefined,
  paneTree: null,
  onAddPane: () => undefined,
  onMenuClosed: () => undefined,
};

type Platform = 'macos' | 'windows' | 'linux';

/** The band as it draws on `platform`, with the panel shown or hidden. */
function draw(platform: Platform, panelOpen: boolean): string {
  vi.stubGlobal('document', { documentElement: { dataset: { platform } } });
  try {
    return renderToStaticMarkup(<TitleBand {...props} panelOpen={panelOpen} />);
  } finally {
    vi.unstubAllGlobals();
  }
}

/** The band's buttons in tab order, by their open tag. */
function buttons(html: string): string[] {
  return [...html.matchAll(/<button [^>]*>/g)].map((m) => m[0]);
}

const attr = (tag: string, name: string) => tag.match(new RegExp(` ${name}="([^"]*)"`))?.[1];

/** The open tag of the one button labeled `label`, and its icon. */
function button(html: string, label: string): { tag: string; svg: string } {
  const at = html.indexOf(`aria-label="${label}"`);
  expect(at, label).toBeGreaterThanOrEqual(0);
  const start = html.lastIndexOf('<button ', at);
  const end = html.indexOf('</button>', at);
  const inner = html.slice(start, end);
  return {
    tag: inner.slice(0, inner.indexOf('>') + 1),
    svg: inner.match(/<svg [^>]*>/)?.[0] ?? '',
  };
}

describe('the Settings button in the title band', () => {
  it('sits after the panel toggle, last on macOS, before the window controls elsewhere', () => {
    const labels = (html: string) => buttons(html).map((b) => attr(b, 'aria-label'));
    // The session button comes first, centered over the terminal.
    expect(labels(draw('macos', true)).slice(1)).toEqual([
      'Add a pane',
      'Search commands',
      'Hide panel',
      'Settings',
    ]);
    expect(labels(draw('macos', false)).slice(1)).toEqual([
      'Search commands',
      'Show panel',
      'Settings',
    ]);
    for (const platform of ['windows', 'linux'] as const) {
      expect(labels(draw(platform, true)).slice(1), platform).toEqual([
        'Add a pane',
        'Search commands',
        'Hide panel',
        'Settings',
        'Minimize',
        'Maximize',
        'Close',
      ]);
      expect(labels(draw(platform, false)).slice(1), platform).toEqual([
        'Search commands',
        'Show panel',
        'Settings',
        'Minimize',
        'Maximize',
        'Close',
      ]);
    }
  });

  it('names each shortcut in aria-keyshortcuts, apart from the name', () => {
    const keys = (platform: 'macos' | 'windows', label: string) =>
      attr(button(draw(platform, true), label).tag, 'aria-keyshortcuts');
    expect(keys('macos', 'Search commands')).toBe('Meta+K');
    expect(keys('windows', 'Search commands')).toBe('Control+K');
    expect(keys('macos', 'Hide panel')).toBe('Meta+Shift+L');
    expect(keys('windows', 'Hide panel')).toBe('Control+Shift+L');
    expect(keys('macos', 'Settings')).toBe('Meta+,');
    expect(keys('windows', 'Settings')).toBe('Control+,');
  });

  it('keeps the tab order the order you see', () => {
    for (const platform of ['macos', 'windows'] as const) {
      for (const tag of buttons(draw(platform, true))) {
        expect(attr(tag, 'tabindex'), tag).toBeUndefined();
      }
    }
  });

  it('names the Settings shortcut in its tooltip, ⌘, on macOS and Ctrl+, elsewhere', () => {
    expect(APP_SHORTCUTS.settings).toBe('Mod+,');
    expect(attr(button(draw('macos', true), 'Settings').tag, 'title')).toBe('Settings (⌘,)');
    expect(attr(button(draw('windows', true), 'Settings').tag, 'title')).toBe('Settings (Ctrl+,)');
    expect(attr(button(draw('linux', false), 'Settings').tag, 'title')).toBe(
      `Settings (${shortcutLabel(APP_SHORTCUTS.settings, false)})`,
    );
  });

  it('is built like the panel toggle beside it', () => {
    for (const [platform, panelOpen] of [
      ['macos', true],
      ['windows', false],
    ] as const) {
      const html = draw(platform, panelOpen);
      const gear = button(html, 'Settings');
      const panel = button(html, panelOpen ? 'Hide panel' : 'Show panel');
      expect(attr(gear.tag, 'type')).toBe('button');
      expect(attr(gear.tag, 'class')).toBe('shell-icon-button');
      // The same 16 px glyph, stroked at 1.25 with round caps and joins.
      expect(gear.svg).toBe(panel.svg);
      expect(gear.svg).toContain('width="16"');
      expect(gear.svg).toContain('stroke-width="1.25"');
      expect(gear.svg).toContain('stroke-linecap="round"');
      expect(gear.svg).toContain('aria-hidden="true"');
    }
  });

  it('shows on every platform with the panel shown or hidden', () => {
    for (const platform of ['macos', 'windows', 'linux'] as const) {
      for (const panelOpen of [true, false]) {
        const html = draw(platform, panelOpen);
        expect(html.match(/aria-label="Settings"/g), `${platform} ${panelOpen}`).toHaveLength(1);
      }
    }
  });
});

// ── Room in the band ─────────────────────────────────────────────────
// The buttons sit at the right end. While the panel shows they sit over
// it, and on Windows and Linux the panel draws wide enough to hold the
// window controls too. With the panel hidden they sit over the terminal
// column, and the session button's equal side insets keep it centered
// and clear of them.

/** The declarations of one rule in frame.css. */
function rule(selector: string): string {
  const at = frameCss.indexOf(`${selector} {`);
  expect(at, selector).toBeGreaterThanOrEqual(0);
  return frameCss.slice(at, frameCss.indexOf('}', at));
}

/** One px value from a rule, the first of a shorthand like padding. */
function px(selector: string, property: string, index = 0): number {
  const found = rule(selector).match(new RegExp(`\\n\\s*${property}:\\s*([^;]+);`));
  expect(found, `${selector} ${property}`).not.toBeNull();
  const parts = (found?.[1] ?? '').trim().split(/\s+/);
  return Number.parseFloat(parts[Math.min(index, parts.length - 1)]);
}

describe('the title band with the Settings button', () => {
  const width = px('.shell-icon-button', 'width');
  const gap = px('.shell-band-actions', 'gap');
  const edge = px('.shell-band-actions', 'right');
  const controls =
    px('.shell-window-controls', 'margin-left') +
    3 * width +
    2 * px('.shell-window-controls', 'gap');

  /** How far the buttons reach in from the right edge of the window. */
  function reach(mac: boolean, panelOpen: boolean): number {
    const n = panelOpen ? 4 : 3;
    const own = edge + n * width + (n - 1) * gap;
    return mac ? own : own + gap + controls;
  }

  const inset = (selector: string) => px(selector, 'padding', 1);
  const shown = inset('.shell-band-title');
  const hiddenMac = inset(".shell[data-panel='hidden'] .shell-band-title");
  const hiddenElsewhere = inset(
    ":root:not([data-platform='macos']) .shell[data-panel='hidden'] .shell-band-title",
  );

  it('measures the buttons as frame.css draws them', () => {
    expect(reach(true, true)).toBe(134);
    expect(reach(false, true)).toBe(238);
    expect(reach(true, false)).toBe(102);
    expect(reach(false, false)).toBe(206);
  });

  it('keeps the session button clear of the buttons with the panel hidden', () => {
    expect(hiddenMac).toBeGreaterThanOrEqual(reach(true, false));
    expect(hiddenElsewhere).toBeGreaterThanOrEqual(reach(false, false));
  });

  it('keeps every button over the panel while it shows, as far in from its edge as the window edge', () => {
    // macOS draws the narrowest panel you can save, and the buttons fit.
    expect(panelWidthFloor(true)).toBe(PANEL_WIDTH_MIN);
    expect(reach(true, true) + edge).toBeLessThanOrEqual(panelWidthFloor(true));
    // Windows and Linux draw it wider, to hold the window controls too.
    expect(panelWidthFloor(false)).toBeGreaterThan(PANEL_WIDTH_MIN);
    expect(reach(false, true) + edge).toBeLessThanOrEqual(panelWidthFloor(false));
    // The terminal keeps 320 px, and at the narrowest window that still
    // leaves the panel its floor.
    expect(tauriConf.app.windows[0].minWidth - 320).toBeGreaterThanOrEqual(panelWidthFloor(false));
  });

  it('leaves the session button room at the narrowest window', () => {
    // The panel never takes the terminal under 320 px, so showing it at
    // the narrowest window leaves 320 less both insets. Hiding the panel
    // never leaves less than that.
    const narrowest = tauriConf.app.windows[0].minWidth;
    const withPanel = 320 - 2 * shown;
    expect(withPanel).toBeGreaterThan(0);
    expect(narrowest - 2 * hiddenMac).toBeGreaterThanOrEqual(withPanel);
    expect(narrowest - 2 * hiddenElsewhere).toBeGreaterThanOrEqual(withPanel);
  });
});

// ── A press ──────────────────────────────────────────────────────────
// React DOM mounts the band on the stand in DOM in src/test/fakeDom.ts,
// which sends no events, so a press calls the button's click handler.

type Handler = (e?: unknown) => void;
const doc = new FakeDocument();
/** What the band listens for on the window. */
const heard = new Map<string, Set<(e: Event) => void>>();
let createRoot: typeof import('react-dom/client').createRoot;

/** The handlers React keeps on an element. */
function on(el: FakeElement): Record<string, Handler> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, Handler>>)[key];
}

/** Mount on the stand in DOM for the tests in the describe that calls
 *  it, and unmount after each. */
function useStandInDom(): (() => Promise<void>)[] {
  const cleanups: (() => Promise<void>)[] = [];

  beforeAll(async () => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    doc.documentElement.dataset.platform = 'macos';
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      addEventListener(type: string, fn: (e: Event) => void) {
        if (!heard.has(type)) heard.set(type, new Set());
        heard.get(type)?.add(fn);
      },
      removeEventListener(type: string, fn: (e: Event) => void) {
        heard.get(type)?.delete(fn);
      },
    });
    vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    // React DOM checks for a DOM once, when it loads.
    ({ createRoot } = await import('react-dom/client'));
  });

  afterEach(async () => {
    for (const cleanup of cleanups.splice(0)) await cleanup();
    doc.activeElement = null;
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });
  return cleanups;
}

describe('pressing the Settings button', () => {
  const cleanups = useStandInDom();

  async function mount() {
    const onOpenSettings = vi.fn();
    const onMenuClosed = vi.fn();
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    await act(async () => {
      root.render(
        createElement(TitleBand, { ...props, panelOpen: true, onOpenSettings, onMenuClosed }),
      );
    });
    cleanups.push(async () => {
      await act(async () => root.unmount());
      doc.body.removeChild(container);
    });
    const [gear] = findAll(container, (el) => el.getAttribute('aria-label') === 'Settings');
    if (!gear) throw new Error('no Settings button');
    const press = () => act(async () => on(gear).onClick({ currentTarget: gear }));
    return { gear, press, onOpenSettings, onMenuClosed };
  }

  it('opens Settings once per press', async () => {
    const m = await mount();
    await m.press();
    expect(m.onOpenSettings).toHaveBeenCalledTimes(1);
    await m.press();
    expect(m.onOpenSettings).toHaveBeenCalledTimes(2);
  });

  it('hands the caret back to the command line when the press left focus on it', async () => {
    // WebView2 and WebKitGTK focus a button on click.
    const m = await mount();
    m.gear.focus();
    await m.press();
    expect(m.onOpenSettings).toHaveBeenCalledTimes(1);
    expect(m.onMenuClosed).toHaveBeenCalledTimes(1);
  });

  it('leaves the caret where it was when the press did not focus it', async () => {
    // WebKit on macOS leaves focus where it was.
    const m = await mount();
    const field = doc.createElement('input');
    field.focus();
    await m.press();
    expect(m.onOpenSettings).toHaveBeenCalledTimes(1);
    expect(m.onMenuClosed).not.toHaveBeenCalled();
    expect(doc.activeElement).toBe(field);
  });

  it('opens Add a pane when Show me asks for it', async () => {
    await mount();
    const add = () => {
      const [button] = findAll(doc.body, (el) => el.getAttribute('aria-label') === 'Add a pane');
      if (!button) throw new Error('no Add a pane button');
      return button;
    };
    expect(add().getAttribute('aria-expanded')).toBe('false');
    expect(heard.get(ADD_PANE_MENU_EVENT)?.size).toBe(1);
    await act(async () => {
      for (const fn of heard.get(ADD_PANE_MENU_EVENT) ?? []) fn(new Event(ADD_PANE_MENU_EVENT));
    });
    expect(add().getAttribute('aria-expanded')).toBe('true');
    const menus = findAll(doc.body, (el) => el.getAttribute('role') === 'menu');
    expect(menus.map((el) => el.getAttribute('aria-label'))).toEqual(['Add a pane']);
  });
});

// ── Add a pane ───────────────────────────────────────────────────────

const pane = (plugin: string, id: string, title: string): LuaPane => ({
  plugin,
  id,
  title,
  meta: '',
  blocks: [],
});

const row = (name: string, on: boolean, stopped: PluginRow['stopped'] = null): PluginRow => ({
  name,
  version: '1.0',
  author: '',
  description: '',
  entry: 'init.lua',
  on,
  stopped,
  loaded_ms: null,
  misnamed: false,
});

/** A panel that shows Affects and Chat, so Map, Group and a second
 *  Chat are left. */
const tree = (...extra: PaneSplit['children']): PaneSplit => ({
  id: 'root',
  split: 'column',
  weight: 1,
  children: [
    { id: 'affects', pane: 'affects', weight: 1, props: {} },
    { id: 'chat', pane: 'chat', weight: 1, props: {} },
    ...extra,
  ],
});

const allShown = (): PaneSplit =>
  tree(
    { id: 'map', pane: 'map', weight: 1, props: {} },
    { id: 'group', pane: 'group', weight: 1, props: {} },
    { id: 'chat-2', pane: 'chat', weight: 1, props: {} },
    { id: 'chat-3', pane: 'chat', weight: 1, props: {} },
    { id: 'chat-4', pane: 'chat', weight: 1, props: {} },
  );

describe('Add a pane', () => {
  const cleanups = useStandInDom();

  // A test before this describe may have opened the menu and read.
  beforeEach(() => {
    lua.reads = 0;
  });

  afterEach(() => {
    lua.panes = new Map();
    lua.rows = null;
    lua.reads = 0;
  });

  /** Weather and Worth, held in the order the plugins drew them. */
  function holdPanes(weather = true, worth = true) {
    lua.panes = new Map([
      ['["worth_pane","worth"]', pane('worth_pane', 'worth', 'Worth')],
      ['["weather_pane","weather"]', pane('weather_pane', 'weather', 'Weather')],
    ]);
    lua.rows = [row('weather_pane', weather), row('worth_pane', worth)];
  }

  async function open(paneTree: PaneSplit) {
    const onAddPane = vi.fn();
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    await act(async () => {
      root.render(createElement(TitleBand, { ...props, panelOpen: true, paneTree, onAddPane }));
    });
    cleanups.push(async () => {
      await act(async () => root.unmount());
      doc.body.removeChild(container);
    });
    const [plus] = findAll(container, (el) => el.getAttribute('aria-label') === 'Add a pane');
    if (!plus) throw new Error('no Add a pane button');
    await act(async () => on(plus).onClick());
    const [menu] = findAll(container, (el) => el.getAttribute('role') === 'menu');
    if (!menu) throw new Error('no Add a pane menu');
    /** Each row's text, with a rule as `---` and the plugin after a
     *  bar, and the note when the menu has no rows. */
    const lines = findAll(
      menu,
      (el) =>
        ['menuitem', 'separator'].includes(el.getAttribute('role') ?? '') ||
        el.getAttribute('class') === 'menu-note',
    ).map((el) => {
      if (el.getAttribute('role') === 'separator') return '---';
      if (el.getAttribute('class') === 'menu-note') return el.textContent;
      const kbd = findAll(el, (k) => k.getAttribute('class') === 'menu-hint')[0];
      const name = findAll(el, (k) => k.getAttribute('class') === 'menu-label')[0];
      return kbd ? `${name.textContent} | ${kbd.textContent}` : el.textContent;
    });
    const pick = (text: string) =>
      act(async () => {
        const item = findAll(
          menu,
          (el) => el.getAttribute('role') === 'menuitem' && el.textContent.startsWith(text),
        )[0];
        on(item).onClick();
      });
    return { lines, pick, onAddPane };
  }

  it('reads the Lua panes only while the menu is open', async () => {
    holdPanes();
    renderToStaticMarkup(<TitleBand {...props} panelOpen paneTree={tree()} />);
    expect(lua.reads).toBe(0);
    await open(tree());
    expect(lua.reads).toBeGreaterThan(0);
  });

  it('lists the Lua panes after a rule, in title order, each with its plugin', async () => {
    holdPanes();
    expect((await open(tree())).lines).toEqual([
      'Map',
      'Group',
      'Chat | starts on Tell',
      '---',
      'Weather | weather_pane',
      'Worth | worth_pane',
    ]);
  });

  it('lists a Lua pane only while its plugin is on and running', async () => {
    holdPanes(false);
    expect((await open(tree())).lines).toEqual([
      'Map',
      'Group',
      'Chat | starts on Tell',
      '---',
      'Worth | worth_pane',
    ]);
    lua.rows = [row('weather_pane', true, 'time'), row('worth_pane', true)];
    expect((await open(tree())).lines).toEqual([
      'Map',
      'Group',
      'Chat | starts on Tell',
      '---',
      'Worth | worth_pane',
    ]);
    lua.rows = null;
    expect((await open(tree())).lines).toEqual(['Map', 'Group', 'Chat | starts on Tell']);
  });

  it('drops a Lua pane once the panel shows it', async () => {
    holdPanes();
    const weather = {
      id: 'weather',
      pane: 'lua',
      weight: 1,
      props: { plugin: 'weather_pane', id: 'weather', title: 'Weather' },
    } as const;
    expect((await open(tree(weather))).lines).toEqual([
      'Map',
      'Group',
      'Chat | starts on Tell',
      '---',
      'Worth | worth_pane',
    ]);
    expect((await open(allShown())).lines).toEqual([
      'Weather | weather_pane',
      'Worth | worth_pane',
    ]);
  });

  it('says every pane is showing only when neither list has one', async () => {
    expect((await open(allShown())).lines).toEqual(['Every pane is showing.']);
    holdPanes(false, false);
    expect((await open(allShown())).lines).toEqual(['Every pane is showing.']);
  });

  it('says a second Chat pane starts on Tell', async () => {
    const solo = (props: Record<string, string>): PaneSplit => ({
      id: 'root',
      split: 'column',
      weight: 1,
      children: [{ id: 'chat', pane: 'chat', weight: 1, props }],
    });
    const empty: PaneSplit = { id: 'root', split: 'column', weight: 1, children: [] };
    expect((await open(empty)).lines).toContain('Chat');
    expect((await open(solo({ rest: '1' }))).lines).toContain('Chat | starts on Tell');
    expect((await open(solo({ channel: 'tell' }))).lines).toContain('Chat');
  });

  it('adds a Lua pane by its plugin, id and title', async () => {
    holdPanes();
    const m = await open(tree());
    await m.pick('Worth');
    expect(m.onAddPane).toHaveBeenCalledWith({
      pane: 'lua',
      props: { plugin: 'worth_pane', id: 'worth', title: 'Worth' },
    });
    const n = await open(tree());
    await n.pick('Map');
    expect(n.onAddPane).toHaveBeenCalledWith({ pane: 'map', props: {} });
  });
});
