import { act, createElement, type ComponentProps } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { LuaBlock, LuaPane } from '../../ipc/panes';
import type { PluginRow } from '../../ipc/scripts';
import { findTheme, themeTokens } from '../../theme/themes';
import { chatInks } from '../chat/chatColors';
import { PaneLeafContext } from '../paneActions';
import type { PaneLeaf } from '../paneLayout';
import { FakeDocument, FakeElement } from '../../test/fakeDom';
import { LuaPane as LuaPaneLeaf, LuaPaneView } from './LuaPane';

// The stores behind the header and its menu reach the Tauri bridge when
// they start. The view draws from plain values and never calls it.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
// What the session holds for the pane and its plugin, and the leaf
// writes the pane makes, for the tests that mount the whole pane.
const session = vi.hoisted(() => ({
  pane: undefined as LuaPane | undefined,
  rows: null as PluginRow[] | null,
}));
vi.mock('../../stores/session/luaPanesStore', async (actual) => ({
  ...(await actual<typeof import('../../stores/session/luaPanesStore')>()),
  useLuaPane: () => session.pane,
}));
vi.mock('../../stores/session/pluginRowsStore', async (actual) => ({
  ...(await actual<typeof import('../../stores/session/pluginRowsStore')>()),
  usePluginRows: () => session.rows,
}));
const updateLeafProps = vi.hoisted(() => vi.fn());
vi.mock('../paneActions', async (actual) => ({
  ...(await actual<typeof import('../paneActions')>()),
  updateLeafProps,
}));
// The last button drawn, so a test can press it.
const pressed = vi.hoisted(() => ({ onClick: undefined as (() => void) | undefined }));
vi.mock('../../ui/Button', async (actual) => {
  const real = await actual<typeof import('../../ui/Button')>();
  return {
    ...real,
    Button: (props: ComponentProps<typeof real.Button>) => {
      pressed.onClick = props.onClick as () => void;
      return createElement(real.Button, props);
    },
  };
});

const theme = findTheme('kanso-zen');
const palette = theme.xterm;
const ground = themeTokens(theme);

const leaf: PaneLeaf = {
  id: 'leaf-weather',
  pane: 'lua',
  weight: 1,
  props: { plugin: 'weather_pane', id: 'weather', title: 'Weather' },
};

const weather = (blocks: LuaBlock[]): LuaPane => ({
  plugin: 'weather_pane',
  id: 'weather',
  title: 'Weather',
  meta: 'Coastal North',
  blocks,
});

const plugin = (patch: Partial<PluginRow>): PluginRow => ({
  name: 'weather_pane',
  version: '1.0.0',
  author: 'Orla',
  description: '',
  entry: 'main.lua',
  on: true,
  stopped: null,
  loaded_ms: null,
  misnamed: false,
  ...patch,
});

function draw(pane: LuaPane | undefined, row: PluginRow | null | undefined = plugin({})) {
  return renderToStaticMarkup(
    <PaneLeafContext.Provider value={leaf}>
      <LuaPaneView
        plugin="weather_pane"
        title="Weather"
        pane={pane}
        row={row}
        palette={palette}
        ground={ground}
        onClose={close}
      />
    </PaneLeafContext.Provider>,
  );
}
const close = vi.fn();

const body = (html: string) => html.slice(html.indexOf('class="pane-body"'));

describe('LuaPaneView', () => {
  it('draws the title, the meta and a row for each label and value', () => {
    const html = draw(
      weather([
        { kind: 'row', label: 'Sky', value: 'rainy' },
        { kind: 'row', label: 'Temperature', value: '60 F' },
      ]),
    );
    expect(html).toContain('<h2 class="pane-label">Weather</h2>');
    expect(html).toContain('<span class="pane-meta">Coastal North</span>');
    expect(body(html)).toContain(
      '<li class="pane-row"><span class="pane-row-name">Sky</span><span class="pane-row-value">rainy</span></li>',
    );
    expect(body(html)).toContain('<span class="pane-row-value">60 F</span>');
  });

  it('fills a gauge with its value over its max, clamped', () => {
    const pct = (value: number, max: number) => {
      const html = draw(weather([{ kind: 'gauge', label: 'Moves', value, max }]));
      return [
        html.match(/pane-member-fill" style="width:(\d+)%"/)?.[1],
        html.match(/pane-member-pct">(\d+)%</)?.[1],
      ];
    };
    expect(pct(30, 40)).toEqual(['75', '75']);
    expect(pct(50, 40)).toEqual(['100', '100']);
    expect(pct(-5, 40)).toEqual(['0', '0']);
    expect(pct(5, 0)).toEqual(['0', '0']);
  });

  it('keeps markup in a line as text', () => {
    const html = draw(weather([{ kind: 'line', text: '<b>loud</b> & clear' }]));
    expect(html).toContain('&lt;b&gt;loud&lt;/b&gt; &amp; clear');
    expect(html).not.toContain('<b>');
  });

  it('draws {red} in the red ink and the rest in the pane color', () => {
    const red = chatInks(palette, ground).red.color;
    const html = draw(weather([{ kind: 'line', text: 'The sky is {red}burning{reset} now.' }]));
    expect(html).toContain(`<span style="color:${red}">burning</span>`);
    expect(html).toContain('<span>The sky is </span>');
    expect(html).toContain('<span> now.</span>');
  });

  it('draws a rule as a hairline', () => {
    expect(draw(weather([{ kind: 'rule' }]))).toContain(
      '<li class="pane-lua-rule" role="separator"></li>',
    );
  });

  it('says how to fill the pane while its plugin is off', () => {
    expect(body(draw(undefined, plugin({ on: false })))).toContain(
      'This pane fills when weather_pane is on. Turn it on under Scripts in Settings.',
    );
  });

  it('says Vosh stopped the plugin', () => {
    expect(body(draw(undefined, plugin({ stopped: 'time' })))).toContain(
      'Vosh stopped weather_pane. Save it under Scripts in Settings or restart Vosh to fill this pane.',
    );
  });

  it('offers to close the pane of a plugin you removed', () => {
    const html = body(draw(undefined, null));
    expect(html).toContain('You removed weather_pane, so nothing fills this pane.');
    expect(html).toContain('class="st-button st-button-secondary">Close pane</button>');
    pressed.onClick?.();
    expect(close).toHaveBeenCalledTimes(1);
  });

  it('leaves the body empty while a plugin that is on has not drawn the pane', () => {
    expect(body(draw(undefined))).toBe('class="pane-body"></div>');
    expect(body(draw(undefined, undefined))).toBe('class="pane-body"></div>');
  });
});

describe('LuaPane', () => {
  const doc = new FakeDocument();
  let createRoot: typeof import('react-dom/client').createRoot;
  const unmounts: (() => Promise<void>)[] = [];

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
    vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    // The theme hooks watch the page for a theme change, which these
    // tests never make.
    vi.stubGlobal(
      'MutationObserver',
      class {
        observe() {}
        disconnect() {}
      },
    );
    // React DOM checks for a DOM once, when it loads.
    ({ createRoot } = await import('react-dom/client'));
  });

  afterEach(async () => {
    for (const unmount of unmounts.splice(0)) await unmount();
    session.pane = undefined;
    session.rows = null;
    updateLeafProps.mockClear();
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  /** Mount the pane in `saved`, a leaf that keeps the title it saved. */
  async function mount(saved: PaneLeaf) {
    const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
    await act(async () => {
      root.render(
        <PaneLeafContext.Provider value={saved}>
          <LuaPaneLeaf />
        </PaneLeafContext.Provider>,
      );
    });
    unmounts.push(async () => {
      await act(async () => root.unmount());
    });
  }

  it('saves the title its plugin draws now into the leaf', async () => {
    session.pane = { ...weather([]), title: 'Weather at sea' };
    session.rows = [plugin({})];
    await mount(leaf);
    expect(updateLeafProps).toHaveBeenCalledTimes(1);
    expect(updateLeafProps).toHaveBeenCalledWith('leaf-weather', { title: 'Weather at sea' });
  });

  it('saves the title of a pane added by split, which has none yet', async () => {
    session.pane = weather([]);
    await mount({ ...leaf, props: { plugin: 'weather_pane', id: 'weather' } });
    expect(updateLeafProps).toHaveBeenCalledWith('leaf-weather', { title: 'Weather' });
  });

  it('leaves the leaf alone when the title matches or the plugin draws nothing', async () => {
    session.pane = weather([]);
    session.rows = [plugin({})];
    await mount(leaf);
    session.pane = undefined;
    session.rows = [plugin({ on: false })];
    await mount(leaf);
    expect(updateLeafProps).not.toHaveBeenCalled();
  });
});
