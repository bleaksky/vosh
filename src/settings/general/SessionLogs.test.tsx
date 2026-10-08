import { act, createElement, type ReactNode } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SettingsPageProps } from '../pageTypes';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../../test/fakeDom';

// The Save as file menu of the log view, mounted on a stand in DOM over a
// fake backend. The menu draws in place, with no page to portal into.

const calls: { cmd: string; args: Record<string, unknown> | undefined }[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: async () => () => undefined,
  emit: async () => undefined,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args?: Record<string, unknown>) => {
    calls.push({ cmd, args });
    if (cmd === 'logs_list_sessions') return [];
    if (cmd === 'logs_search_page') return { hits: [], total: 0 };
    if (cmd === 'logs_save') {
      const options = args?.options as { format: string };
      const ext = { text: 'txt', ansi: 'log', html: 'html' }[options.format];
      return `Vosh log, last 7 days.${ext}`;
    }
    if (cmd === 'sessions_list') return [];
    return null;
  },
}));

vi.mock('../../ui/MenuSurface', async (actual) => ({
  ...(await actual<typeof import('../../ui/MenuSurface')>()),
  MenuSurface: ({ label, children }: { label: string; children: ReactNode }) => (
    <menu aria-label={label}>{children}</menu>
  ),
}));

const settle = (ms = 0) => new Promise((resolve) => setTimeout(resolve, ms));
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
    setTimeout,
    clearTimeout,
    innerWidth: 1200,
    innerHeight: 800,
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('Node', FakeNode);
  vi.stubGlobal('Element', FakeElement);
  vi.stubGlobal('HTMLElement', FakeElement);
  vi.stubGlobal('getComputedStyle', () => ({ getPropertyValue: () => ' #646260' }));
  vi.stubGlobal('localStorage', {
    getItem: () => null,
    setItem: () => undefined,
    removeItem: () => undefined,
  });
  ({ createRoot } = await import('react-dom/client'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

const cleanups: (() => Promise<void>)[] = [];
afterEach(async () => {
  for (const clean of cleanups.splice(0)) await clean();
});

beforeEach(() => {
  calls.length = 0;
});

/** The React props of a stand in element. */
function props<T>(el: FakeElement): T {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
  return (el as unknown as Record<string, T>)[key];
}

async function draw() {
  const { SessionLogs } = await import('./SessionLogs');
  const errors: string[] = [];
  const page: SettingsPageProps & { onSaveScene: (log: number) => void } = {
    target: { group: 'general', section: 'logs' },
    navSeq: 1,
    config: null,
    setConfig: () => undefined,
    onError: (message) => errors.push(message),
    pathB: false,
    navigate: () => undefined,
    setLeaveGuard: () => undefined,
    onSaveScene: () => undefined,
  };
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    root.render(createElement(SessionLogs, page));
    await settle(20);
  });
  cleanups.push(async () => {
    await act(async () => root.unmount());
  });
  const run = async (fn: () => void) => {
    await act(async () => {
      fn();
      await settle(20);
    });
  };
  const items = () => findAll(container, (el) => el.getAttribute('role')?.startsWith('menuitem'));
  const item = (text: string) => items().find((el) => el.textContent === text) as FakeElement;
  return {
    errors,
    openMenu: () =>
      run(() => {
        const save = findAll(container, (el) => el.getAttribute('aria-label') === 'Save as file');
        props<{ onClick: (e: unknown) => void }>(save[0]).onClick({
          currentTarget: {
            getBoundingClientRect: () => ({ left: 600, right: 628, top: 20, bottom: 48 }),
          },
        });
      }),
    rows: () =>
      findAll(
        container,
        (el) =>
          el.getAttribute('role') === 'separator' ||
          !!el.getAttribute('role')?.startsWith('menuitem'),
      ).map((el) => (el.getAttribute('role') === 'separator' ? '—' : el.textContent)),
    checked: (text: string) => item(text).getAttribute('aria-checked'),
    pick: (text: string) => run(() => props<{ onClick: () => void }>(item(text)).onClick()),
    count: () =>
      findAll(container, (el) => el.getAttribute('class') === 'st-logs-count')[0]?.textContent,
  };
}

const saves = () =>
  calls
    .filter((c) => c.cmd === 'logs_save')
    .map((c) => c.args as { options: Record<string, unknown>; name: string });

describe('Save as file', () => {
  it('offers times above the three kinds of file, off until you check it', async () => {
    const page = await draw();
    await page.openMenu();
    expect(page.rows()).toEqual([
      'Include times',
      '—',
      'Plain text (.txt)',
      'With colors (.log)',
      'Web page (.html)',
    ]);
    expect(page.checked('Include times')).toBe('false');
    await page.pick('Include times');
    expect(page.checked('Include times')).toBe('true');
    expect(saves()).toEqual([]);
  });

  it('saves a web page in the theme in front and names the file', async () => {
    const page = await draw();
    await page.openMenu();
    await page.pick('Web page (.html)');
    const [saved] = saves();
    expect(saved?.name).toBe('Vosh log, last 7 days');
    const options = saved?.options as {
      format: string;
      times: boolean;
      palette: { muted: string; ansi: string[] };
    };
    expect(options.format).toBe('html');
    expect(options.times).toBe(false);
    expect(options.palette.muted).toBe('#646260');
    expect(options.palette.ansi).toHaveLength(16);
    expect(page.count()).toBe('Saved Vosh log, last 7 days.html in Downloads');
  });

  it('starts each line with its time once you check Include times', async () => {
    const page = await draw();
    await page.openMenu();
    await page.pick('Include times');
    await page.pick('Plain text (.txt)');
    expect(saves().map((s) => s.options)).toEqual([{ format: 'text', times: true, palette: null }]);
    expect(page.errors).toEqual([]);
  });
});
