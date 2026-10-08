import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeDocument, FakeElement, findAll } from '../../test/fakeDom';
import type { GetStartedFacts } from './steps';

// Get started in the window, boards 1 and 2 of First Run: the card at
// launch, how it folds under the prompt card, at Esc and at Connect,
// Close with its toast, and the Chat step's switch. The card mounts over
// a fake Tauri, so the store, the escape stack and the preset plan run
// as the window runs them.

type Handler = (event: { payload: unknown }) => void;
const bus = vi.hoisted(() => ({
  saved: null as unknown,
  enabled: ['none'] as string[],
  calls: [] as [string, unknown][],
  keydown: null as ((event: unknown) => void) | null,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (_event: string, _cb: Handler) => () => undefined,
  emit: async () => undefined,
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args?: unknown) => {
    if (cmd === 'get_started_get') return bus.saved;
    if (cmd === 'ui_get_config') return { enabled_presets: bus.enabled };
    if (cmd === 'preset_edits_get') return {};
    if (cmd === 'triggers_list' || cmd === 'macros_list') return [];
    bus.calls.push([cmd, args]);
    if (cmd === 'presets_enabled_set') return { installed: 0, removed: [] };
    return null;
  },
}));

// The facts the steps read live, held still here.
const facts = vi.hoisted(() => ({
  value: {
    character: null,
    enabledPresets: ['none'],
    panes: ['map', 'affects'],
    tracked: 0,
    promptPlace: null,
  } as GetStartedFacts,
}));
vi.mock('./useGetStartedFacts', () => ({ useGetStartedFacts: () => facts.value }));

const doc = new FakeDocument();
let act: typeof import('react').act;
let createRoot: typeof import('react-dom/client').createRoot;
let GetStarted: typeof import('./GetStarted').GetStarted;
let store: typeof import('./getStartedStore');
let toasts: typeof import('../../stores/toasts');

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => undefined });
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    innerHeight: 800,
    innerWidth: 1280,
    addEventListener(type: string, cb: (event: unknown) => void) {
      if (type === 'keydown') bus.keydown = cb;
    },
    removeEventListener() {},
    setTimeout: globalThis.setTimeout.bind(globalThis),
    clearTimeout: globalThis.clearTimeout.bind(globalThis),
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: 'MacIntel' });
  vi.stubGlobal(
    'MutationObserver',
    class {
      observe() {}
      disconnect() {}
    },
  );
});

/** Fresh modules for each mount, so each starts from a store that has
 *  read nothing yet. */
async function load() {
  vi.resetModules();
  ({ act } = await import('react'));
  ({ createRoot } = await import('react-dom/client'));
  ({ GetStarted } = await import('./GetStarted'));
  store = await import('./getStartedStore');
  toasts = await import('../../stores/toasts');
}

beforeEach(() => {
  bus.calls = [];
  bus.enabled = ['none'];
  facts.value = { ...facts.value, enabledPresets: ['none'] };
});

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
});

/** The handlers React keeps on an element. */
function reactProps(el: FakeElement): Record<string, (e?: unknown) => void> {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  if (!key) throw new Error('the element has no React props');
  return (el as unknown as Record<string, Record<string, (e?: unknown) => void>>)[key];
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Mount Get started over what profiles.toml keeps. */
async function mount(saved: unknown) {
  bus.saved = saved;
  await load();
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  let focused = 0;
  const props = {
    play: { live: false, character: null, connect: () => undefined },
    covered: false,
    host: { terminal: () => null, area: () => null, dock: () => null },
    cell: null,
    show: null,
    onShowMe: () => undefined,
    focusInput: () => {
      focused += 1;
    },
  };
  const render = (change: Partial<typeof props>) =>
    act(async () => {
      Object.assign(props, change);
      root.render(<GetStarted {...props} />);
      await settle();
    });
  await render({});
  await act(settle);
  cleanups.push(async () => {
    await act(async () => root.unmount());
    doc.body.removeChild(container);
    for (const t of toasts.getToasts()) toasts.dismissToast(t.id);
  });
  const card = () => findAll(container, (el) => el.getAttribute('aria-label') === 'Get started')[0];
  const press = (text: string) =>
    act(async () => {
      const button = findAll(
        container,
        (el) =>
          el.nodeName === 'BUTTON' &&
          (el.textContent.startsWith(text) || el.getAttribute('aria-label') === text),
      )[0];
      if (!button) throw new Error(`no ${text}`);
      reactProps(button).onClick({ currentTarget: button, preventDefault() {} });
      await settle();
    });
  return {
    card,
    text: () => card()?.textContent ?? '',
    focused: () => focused,
    render,
    press,
    /** Press Esc on the command line. */
    escape: () =>
      act(async () => {
        bus.keydown?.({
          key: 'Escape',
          isComposing: false,
          target: null,
          preventDefault() {},
          stopPropagation() {},
        });
        await settle();
      }),
    /** Flip the switch named `name`, as a click does. */
    flip: (name: string) =>
      act(async () => {
        const input = findAll(container, (el) => el.getAttribute('aria-label') === name)[0];
        if (!input) throw new Error(`no switch ${name}`);
        const checked = (input as unknown as { checked: boolean }).checked;
        reactProps(input).onChange({ target: { checked: !checked } });
        await settle();
        await settle();
      }),
  };
}

describe('Get started', () => {
  it('opens at launch on the list, with the connect step in focus', async () => {
    const view = await mount({ atLaunch: true, done: [] });
    expect(view.card()?.getAttribute('role')).toBe('region');
    const text = view.text();
    expect(text).toContain('Get started5 steps');
    expect(text).toContain('Vosh starts plain, with every preset off.');
    expect(text).toContain('Color what the game prints5 suggested');
    expect(text).toContain('After you log in');
    expect(text).toContain('New session…Connect');
    expect(text).toContain('Find it again with Get started in the palette.');
  });

  it('counts what you finished', async () => {
    const view = await mount({ atLaunch: true, done: ['connect'] });
    expect(view.text()).toContain('Get started1 of 5 done');
  });

  it('folds when the prompt card opens, so the two never show together', async () => {
    const view = await mount({ atLaunch: true, done: [] });
    await view.render({ covered: true });
    expect(view.card()).toBeUndefined();
    expect(store.getGetStarted().shows).toBe('folded');
  });

  it('folds at Esc and hands the caret back', async () => {
    const view = await mount({ atLaunch: true, done: [] });
    await view.escape();
    expect(store.getGetStarted().shows).toBe('folded');
    expect(view.card()).toBeUndefined();
    expect(view.focused()).toBe(1);
  });

  it('folds the moment Connect dials', async () => {
    const view = await mount({ atLaunch: true, done: [] });
    await view.render({ play: { live: true, character: null, connect: () => undefined } });
    expect(store.getGetStarted().shows).toBe('folded');
  });

  it('ends at Close and says Help opens it again', async () => {
    const view = await mount({ atLaunch: true, done: [] });
    await view.press('Close');
    expect(store.getGetStarted().shows).toBe('shut');
    expect(bus.calls).toContainEqual(['get_started_set', { atLaunch: false, done: [] }]);
    expect(toasts.getToasts().at(-1)).toMatchObject({
      message: 'Get started closed',
      meta: 'Help opens it again',
    });
  });

  it('opens the Chat step with Tells you send and its sample', async () => {
    const view = await mount({ atLaunch: true, done: [] });
    await view.press('Add Chat and Group');
    const text = view.text();
    expect(text).toContain('BackAdd Chat and Group');
    expect(text).toContain('Puts each tell you send in the chat pane, beside the ones you get.');
    expect(text).toContain('[tell]to Tolliver:');
    expect(text).toContain('Show meNext step');
    await view.flip('Tells you send');
    expect(bus.calls.filter(([cmd]) => cmd === 'presets_enabled_set')).toHaveLength(1);
    expect(bus.calls.find(([cmd]) => cmd === 'presets_enabled_set')?.[1]).toMatchObject({
      changes: [{ id: 'sent_tells', on: true }],
    });
  });

  it('reads as a summary once every step is done', async () => {
    facts.value = { ...facts.value, character: 'Orla' };
    const view = await mount({
      atLaunch: false,
      done: ['connect', 'presets', 'panes', 'affects', 'prompt'],
    });
    store.openList();
    await view.render({});
    const text = view.text();
    expect(text).toContain('Get started5 of 5 done');
    expect(text).toContain('Every step is done. Settings changes any of it.');
    expect(text).toContain('Connect to The Forsaken LandsOrla');
    expect(text).not.toContain('After you log in');
    expect(text).toContain('Get started stays in Help.Done');
  });
});
