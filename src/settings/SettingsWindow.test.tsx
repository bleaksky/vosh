import { act, createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SETTINGS_PENDING_KEY } from '../lib/settingsLink';
import type { SettingsPageProps } from './pageTypes';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../test/fakeDom';
import { SettingsWindow } from './SettingsWindow';

// The frame's breadcrumb, drawn once on a cold open to the target the
// main window left. Effects do not run in a markup render, so nothing
// reaches the app. The crumb is the frame's, so the Scripts page under
// it draws nothing here.

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve(null)) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ show: async () => undefined, setFocus: async () => undefined }),
}));
vi.mock('./scripts/ScriptsPage', () => ({ ScriptsPage: () => null }));
// The pages stand in as one element that names the target they got.
vi.mock('./general/GeneralPage', () => ({ GeneralPage: shownTarget }));
vi.mock('./automation/AutomationPage', () => ({ AutomationPage: shownTarget }));
// The window's own close and controls reach the app, so they stand down.
vi.mock('./useSettingsClose', () => ({ useSettingsClose: () => undefined }));
vi.mock('../ui/WindowControls', () => ({ WindowControls: () => null }));

function shownTarget({ target }: SettingsPageProps) {
  return createElement('p', { 'data-shown': `${target.group}:${target.section ?? ''}` });
}

const stored = new Map<string, string>();

beforeEach(() => {
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => stored.get(key) ?? null,
    setItem: (key: string, value: string) => void stored.set(key, value),
    removeItem: (key: string) => void stored.delete(key),
  });
  vi.stubGlobal('navigator', { userAgent: 'Macintosh', platform: 'MacIntel' });
});

afterEach(() => {
  stored.clear();
  vi.unstubAllGlobals();
});

/** The breadcrumb a cold open to `link` draws. */
function crumb(link: string): string {
  stored.set(SETTINGS_PENDING_KEY, link);
  const html = renderToStaticMarkup(<SettingsWindow />);
  return /<div class="st-crumb"[^>]*>.*?<\/h1>/.exec(html)?.[0] ?? '';
}

describe('the Settings breadcrumb', () => {
  it('titles a plugin page with its name and links back to Scripts', () => {
    const html = crumb('scripts:Vitals_Alert');
    expect(html).toMatch(/<a href="#scripts" class="st-crumb-link">Scripts<\/a>/);
    expect(html).toMatch(/<h1 class="st-crumb-title"[^>]*>Vitals_Alert<\/h1>/);
  });

  it('titles the Scripts list with the group alone', () => {
    const html = crumb('scripts');
    expect(html).not.toContain('st-crumb-link');
    expect(html).toMatch(/<h1 class="st-crumb-title"[^>]*>Scripts<\/h1>/);
  });
});

// The Settings keys, Mod and Shift with 1 to 4, open their page in this
// window too. The window is mounted on a stand-in DOM and the key is
// pressed on its document.
describe('the Settings keys', () => {
  async function mount(platform: { userAgent: string; platform: string }) {
    const doc = new FakeDocument();
    const keys = new Set<(event: unknown) => void>();
    Object.assign(doc, {
      addEventListener: (type: string, cb: (event: unknown) => void) => {
        if (type === 'keydown') keys.add(cb);
      },
      removeEventListener: (_type: string, cb: (event: unknown) => void) => keys.delete(cb),
    });
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('document', doc);
    vi.stubGlobal('window', {
      document: doc,
      location: { protocol: 'about:' },
      HTMLIFrameElement: class {},
      addEventListener() {},
      removeEventListener() {},
      setTimeout: () => 0,
      clearTimeout() {},
    });
    vi.stubGlobal('navigator', platform);
    vi.stubGlobal('Node', FakeNode);
    vi.stubGlobal('Element', FakeElement);
    vi.stubGlobal('HTMLElement', FakeElement);
    const { createRoot } = await import('react-dom/client');
    const host = doc.createElement('div');
    doc.body.appendChild(host);
    const root = createRoot(host as unknown as HTMLElement);
    await act(async () => root.render(createElement(SettingsWindow)));
    const shown = () =>
      findAll(host, (el) => el.hasAttribute('data-shown'))[0]?.getAttribute('data-shown');
    const press = async (init: Record<string, unknown>) => {
      const event = {
        key: '',
        code: '',
        metaKey: false,
        ctrlKey: false,
        shiftKey: false,
        altKey: false,
        repeat: false,
        defaultPrevented: false,
        preventDefault() {
          this.defaultPrevented = true;
        },
        ...init,
      };
      await act(async () => {
        for (const key of keys) key(event);
      });
      return event;
    };
    return { root, shown, press };
  }

  it('opens Aliases on Ctrl+Shift+2 on Windows and Linux', async () => {
    const { root, shown, press } = await mount({ userAgent: 'Windows NT', platform: 'Win32' });
    expect(shown()).toBe('general:');
    const event = await press({ key: '@', code: 'Digit2', ctrlKey: true, shiftKey: true });
    expect(event.defaultPrevented).toBe(true);
    expect(shown()).toBe('automation:aliases');
    act(() => root.unmount());
  });

  it('opens Timers on Cmd+Shift+4 on macOS, and leaves other keys alone', async () => {
    const { root, shown, press } = await mount({ userAgent: 'Macintosh', platform: 'MacIntel' });
    // Ctrl belongs to your macros on macOS, and Shift with 5 is no key.
    const ctrl = await press({ key: '$', code: 'Digit4', ctrlKey: true, shiftKey: true });
    const five = await press({ key: '%', code: 'Digit5', metaKey: true, shiftKey: true });
    expect(ctrl.defaultPrevented).toBe(false);
    expect(five.defaultPrevented).toBe(false);
    expect(shown()).toBe('general:');
    await press({ key: '$', code: 'Digit4', metaKey: true, shiftKey: true });
    expect(shown()).toBe('automation:timers');
    act(() => root.unmount());
  });
});
