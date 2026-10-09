import { act, createElement } from 'react';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { FONT_CHANGED, SESSIONS_CHANGED, UI_CONFIG_REPLACED } from '../ipc/events';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import type { SnoopLook } from './useSnoopWindowLook';

// The look of the snoop window of session 2, which plays Builder while
// session 1, on Staff, sits in front. The backend and the theme
// registries are faked, and a probe keeps the look the hook returns.

type Handler = (event: { payload: unknown }) => void;
const fake = vi.hoisted(() => ({
  handlers: new Map<string, Set<(event: { payload: unknown }) => void>>(),
  asked: [] as (string | null | undefined)[],
  fonts: { Staff: 'Menlo', Builder: 'Iosevka' } as Record<string, string>,
  look: null as SnoopLook | null,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, cb: Handler) => {
    let set = fake.handlers.get(event);
    if (!set) fake.handlers.set(event, (set = new Set()));
    set.add(cb);
    return Promise.resolve(() => set.delete(cb));
  },
  emit: () => Promise.resolve(),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args: { profile?: string | null } = {}) => {
    if (cmd !== 'ui_get_config') return Promise.resolve([]);
    fake.asked.push(args.profile);
    return Promise.resolve({ font_family: fake.fonts[args.profile ?? 'Staff'], font_size: 14 });
  },
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ show: () => Promise.resolve(), setFocus: () => Promise.resolve() }),
}));
vi.mock('../lib/fontLoader', () => ({
  loadFontStack: () => undefined,
  renderFontStack: (family: string) => family,
}));
vi.mock('../lib/reveal', () => ({ showAfterThemePaint: (show: () => void) => show() }));
vi.mock('../theme/baseAnsi', async (actual) => ({
  ...(await actual<typeof import('../theme/baseAnsi')>()),
  setBaseAnsi: () => {},
}));
vi.mock('../theme/fitGameColors', async (actual) => ({
  ...(await actual<typeof import('../theme/fitGameColors')>()),
  setColorVision: () => {},
  setFitGameColors: () => {},
}));
vi.mock('../theme/theme', async (actual) => ({
  ...(await actual<typeof import('../theme/theme')>()),
  applyThemePrefs: () => {},
}));
vi.mock('../theme/themes', async (actual) => ({
  ...(await actual<typeof import('../theme/themes')>()),
  setCustomThemes: () => {},
}));
vi.mock('./useUiConfigFollow', () => ({ DEFAULT_FONT_FAMILY: 'Default' }));

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
    setTimeout: () => 1,
    clearTimeout: () => {},
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  ({ createRoot } = await import('react-dom/client'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  fake.asked.length = 0;
  fake.look = null;
});

const fire = (event: string, payload: unknown) =>
  act(async () => {
    for (const cb of fake.handlers.get(event) ?? []) cb({ payload });
  });

const rows = (builder: string) =>
  [1, 2].map((id) => ({
    id,
    name: null,
    character: id === 1 ? 'Tolliver' : 'Orla',
    host: 'play.theforsakenlands.com',
    port: 1848,
    tls: false,
    profile: id === 1 ? 'Staff' : builder,
    connected: true,
    since: null,
    selected: id === 1,
  }));

async function open(session: number) {
  const { useSnoopWindowLook } = await import('./useSnoopWindowLook');
  function Probe() {
    fake.look = useSnoopWindowLook(session);
    return null;
  }
  const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
  await act(async () => root.render(createElement(Probe)));
  return () => act(async () => root.unmount());
}

describe('the snoop window look', () => {
  it('draws with the profile its session plays, not the one in front', async () => {
    const close = await open(2);
    expect(fake.asked).toEqual([]);
    await fire(SESSIONS_CHANGED, rows('Builder'));
    expect(fake.asked).toEqual(['Builder']);
    expect(fake.look?.fontFamily).toBe('Iosevka');

    // A font change for the profile in front reads Builder again.
    await fire(FONT_CHANGED, { family: 'Menlo', size: 18 });
    expect(fake.asked).toEqual(['Builder', 'Builder']);
    expect(fake.look?.fontFamily).toBe('Iosevka');
    expect(fake.look?.fontSize).toBe(14);

    await fire(UI_CONFIG_REPLACED, null);
    expect(fake.asked).toEqual(['Builder', 'Builder', 'Builder']);

    // The session moves to Staff, so the window takes its look.
    await fire(SESSIONS_CHANGED, rows('Staff'));
    expect(fake.asked.at(-1)).toBe('Staff');
    expect(fake.look?.fontFamily).toBe('Menlo');
    await close();
  });
});
