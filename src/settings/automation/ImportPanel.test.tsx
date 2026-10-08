import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { MigrationPlan } from '../../ipc/wizard';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../../test/fakeDom';

// The shared catalog preview as it runs, opened from Preview… under
// Automation, then Import. The panel mounts on a stand-in DOM with the
// two migration commands faked, so the plan read, the picks, Apply, Esc
// and the focus moves all run. The markup itself is in
// MigrationWizard.test.tsx.

const calls: { cmd: string; args: Record<string, unknown> | undefined }[] = [];
let finishApply: (() => void) | null = null;

vi.mock('@tauri-apps/api/event', () => ({
  listen: async () => () => undefined,
  emit: async () => undefined,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    calls.push({ cmd, args });
    if (cmd === 'migration_analyze') return Promise.resolve(PLAN);
    if (cmd === 'migration_apply')
      return new Promise<void>((resolve) => {
        finishApply = resolve;
      });
    return Promise.resolve(null);
  },
}));

// Maren's F1 looks where Orla's scans, and the analyzer keeps Orla's.
const PLAN: MigrationPlan = {
  source_profiles: ['Maren', 'Orla'],
  auto_resolved: { aliases: [], triggers: [], macros: [] },
  conflicts: [
    {
      kind: 'macro',
      name: 'F1',
      default_source: 'Orla',
      variants: [
        {
          source_profile: 'Maren',
          switched_on: true,
          item: { kind: 'macro', item: { key: 'F1', command: 'look' } },
        },
        {
          source_profile: 'Orla',
          switched_on: true,
          item: { kind: 'macro', item: { key: 'F1', command: 'scan' } },
        },
      ],
    },
  ],
  loadouts: [
    { name: 'Maren', enabled_groups: [] },
    { name: 'Orla', enabled_groups: [] },
  ],
  shared_presets: [],
  profile_presets: [[], []],
};

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

const doc = new FakeDocument();
// The escape stack listens on the window, so this one keeps its keys.
const keys = new Set<(event: unknown) => void>();
let createRoot: typeof import('react-dom/client').createRoot;
let ImportPanel: typeof import('./ImportPanel').ImportPanel;

/** The first control a dialog's focus goes to, as DIALOG_FOCUSABLE
 *  picks it, for the buttons this panel's dialog holds. */
function firstControl(this: FakeElement): FakeElement | null {
  return findAll(this, (el) => el.nodeName === 'BUTTON' && !el.hasAttribute('disabled'))[0] ?? null;
}

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener: (type: string, cb: (event: unknown) => void) => {
      if (type === 'keydown') keys.add(cb);
    },
    removeEventListener: (_type: string, cb: (event: unknown) => void) => keys.delete(cb),
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  Object.assign(FakeElement.prototype, { querySelector: firstControl });
  ({ createRoot } = await import('react-dom/client'));
  ({ ImportPanel } = await import('./ImportPanel'));
});

afterAll(() => {
  delete (FakeElement.prototype as unknown as Record<string, unknown>).querySelector;
  vi.unstubAllGlobals();
});

beforeEach(() => {
  calls.length = 0;
  finishApply = null;
  doc.activeElement = null;
});

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
});

type Props = Record<string, (event?: unknown) => void>;
const props = (el: FakeElement): Props => {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
  return (el as unknown as Record<string, Props>)[key];
};

async function mount() {
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    root.render(createElement(ImportPanel, { onError: () => undefined }));
    await settle();
  });
  cleanups.push(async () => {
    await act(async () => root.unmount());
  });
  const button = (text: string) =>
    findAll(container, (el) => el.nodeName === 'BUTTON' && el.textContent === text)[0];
  const layer = () =>
    findAll(container, (el) => (el.getAttribute('class') ?? '').includes('ov-wizard-layer'))[0];
  return {
    button,
    open: () => layer() !== undefined,
    press: async (el: FakeElement) => {
      await act(async () => {
        props(el).onClick();
        await settle();
      });
    },
    escape: async () => {
      const event = {
        key: 'Escape',
        isComposing: false,
        target: doc.body,
        preventDefault() {},
        stopPropagation() {},
      };
      await act(async () => {
        for (const cb of keys) cb(event);
        await settle();
      });
    },
    pressOutside: async () => {
      const el = layer();
      await act(async () => {
        props(el).onPointerDown({ target: el, currentTarget: el });
        await settle();
      });
    },
  };
}

describe('the shared catalog preview', () => {
  it('starts on the first pick once the plan loads, and Esc gives focus back to Preview…', async () => {
    const m = await mount();
    await m.press(m.button('Preview…'));
    expect(m.open()).toBe(true);
    expect(calls.map((c) => c.cmd)).toEqual(['migration_analyze']);
    expect(doc.activeElement?.textContent).toBe('Maren');
    // The analyzer's default starts picked.
    expect(m.button('Orla').getAttribute('aria-pressed')).toBe('true');

    await m.escape();
    expect(m.open()).toBe(false);
    expect(doc.activeElement).toBe(m.button('Preview…'));
  });

  it('sends the version you pick, and Esc and a press outside wait while Apply runs', async () => {
    const m = await mount();
    await m.press(m.button('Preview…'));
    await m.press(m.button('Maren'));
    expect(m.button('Maren').getAttribute('aria-pressed')).toBe('true');

    await m.press(m.button('Apply'));
    expect(calls.at(-1)).toEqual({
      cmd: 'migration_apply',
      args: {
        resolutions: [{ kind: 'macro', name: 'F1', source_profile: 'Maren' }],
        library: expect.any(Array),
      },
    });
    expect(m.button('Applying…')).toBeDefined();
    await m.escape();
    await m.pressOutside();
    expect(m.open()).toBe(true);

    await act(async () => {
      finishApply?.();
      await settle();
    });
    expect(doc.activeElement?.textContent).toBe('Close');
    await m.pressOutside();
    expect(m.open()).toBe(false);
    expect(doc.activeElement).toBe(m.button('Preview…'));
  });
});
