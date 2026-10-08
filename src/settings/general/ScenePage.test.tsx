import { act, createElement } from 'react';
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LogSession, ScenePreview, ScenePreviewLine } from '../../ipc/logs';
import type { SettingsPageProps } from '../pageTypes';
import { FakeDocument, FakeElement, FakeNode, findAll } from '../../test/fakeDom';

// Save a scene, board 5 of the Alerts and Scenes review, mounted on a
// stand in DOM over a fake backend. The lines are Thickening Woods from
// fixtures/room-colors/looks.json and the say and tell of lines.json.

const calls: { cmd: string; args: Record<string, unknown> | undefined }[] = [];
let failSave = false;

const LOG: LogSession = {
  id: 7,
  host: 'play.theforsakenlands.com',
  port: 1848,
  started_at_ms: new Date(2026, 9, 3, 21, 2).getTime(),
  ended_at_ms: new Date(2026, 9, 3, 21, 16).getTime(),
  line_count: 1284,
};

const line = (id: number, minute: number, text: string, out: string | null): ScenePreviewLine => ({
  id,
  ts_ms: new Date(2026, 9, 3, 21, minute, 1).getTime(),
  text,
  raw: null,
  out,
});

function preview(filter: { prompts: boolean }): ScenePreview {
  const lines = [
    line(101, 14, 'Thickening Woods', null),
    line(102, 14, 'Maren walks in.', null),
    line(103, 14, '<1020hp 800m 930mv> ', filter.prompts ? null : 'prompt'),
    line(104, 15, "Tolliver says 'The day has begun.'", null),
    line(105, 15, "Tolliver tells you '[Exits: north east south west]'", 'tell'),
  ];
  return {
    lines,
    total: lines.length,
    kept: lines.filter((l) => l.out === null).length,
    capped: false,
    older: false,
    file_name: 'Thickening Woods, October 3.html',
  };
}

vi.mock('@tauri-apps/api/event', () => ({
  listen: async () => () => undefined,
  emit: async () => undefined,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args?: Record<string, unknown>) => {
    calls.push({ cmd, args });
    if (cmd === 'logs_list_sessions') return [LOG];
    if (cmd === 'scene_preview') return preview(args?.filter as { prompts: boolean });
    if (cmd === 'scene_save') {
      if (failSave) throw new Error('Vosh could not save the scene in your Downloads folder.');
      return 'Thickening Woods, October 3.html';
    }
    if (cmd === 'sessions_list') return [];
    return null;
  },
}));

const settle = (ms = 0) => new Promise((resolve) => setTimeout(resolve, ms));

/** Wait for the page to read the preview `n` times in all, and for that
 *  read to land. Each wait is an act of its own, so React commits what
 *  came in before the next one. */
async function previews(n: number) {
  for (let i = 0; i < 100 && calls.filter((c) => c.cmd === 'scene_preview').length < n; i++) {
    await act(async () => {
      await settle(20);
    });
  }
  await act(async () => {
    await settle();
  });
}

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
  failSave = false;
});

/** The React props of a stand in element. */
function props<T>(el: FakeElement): T {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
  return (el as unknown as Record<string, T>)[key];
}

async function draw(log: number | null) {
  const { ScenePage } = await import('./ScenePage');
  const errors: (string | null)[] = [];
  const moves: unknown[] = [];
  const page: SettingsPageProps & { log: number | null } = {
    target: { group: 'general', section: 'scene' },
    navSeq: 1,
    config: null,
    setConfig: () => undefined,
    onError: (message) => errors.push(message),
    pathB: false,
    navigate: (target) => moves.push(target),
    setLeaveGuard: () => undefined,
    log,
  };
  const container = doc.createElement('div');
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    root.render(createElement(ScenePage, page));
    await settle();
  });
  await previews(1);
  cleanups.push(async () => {
    await act(async () => root.unmount());
  });
  const byClass = (name: string) =>
    findAll(container, (el) => (el.getAttribute('class') ?? '').split(' ').includes(name));
  const button = (text: string) =>
    findAll(container, (el) => el.nodeName === 'BUTTON' && el.textContent === text)[0];
  return {
    errors,
    moves,
    rows: () =>
      byClass('st-logs-line').map((li) => ({
        text: findAll(li, (el) => el.getAttribute('class') === 'st-logs-text')[0]?.textContent,
        why: findAll(li, (el) => el.getAttribute('class') === 'sc-why')[0]?.textContent ?? null,
        out: (li.getAttribute('class') ?? '').includes('is-out'),
      })),
    count: () => byClass('st-meta')[0]?.textContent,
    status: () => byClass('st-savebar-status')[0]?.textContent,
    chips: () => byClass('st-chip-label').map((el) => el.textContent),
    act: async (run: () => void, read = true) => {
      const before = calls.filter((c) => c.cmd === 'scene_preview').length;
      await act(async () => {
        run();
        await settle(20);
      });
      if (read) await previews(before + 1);
    },
    button,
    times: () => byClass('sc-time-pick'),
    switches: () => findAll(container, (el) => el.getAttribute('role') === 'switch'),
    labelled: (label: string) =>
      findAll(container, (el) => el.getAttribute('aria-label') === label)[0],
  };
}

const named = (cmd: string) => calls.filter((c) => c.cmd === cmd).map((c) => c.args);

describe('Save a scene', () => {
  it('opens on the log you picked with the first filter, what stays out drawn quiet', async () => {
    const page = await draw(7);
    expect(named('scene_preview')[0]).toEqual({
      range: {
        log: 7,
        fromMs: new Date(2026, 9, 3, 21, 2).getTime(),
        toMs: new Date(2026, 9, 3, 21, 16, 59, 999).getTime(),
      },
      filter: {
        prompts: false,
        commands: false,
        leftOut: ['tell', 'newbie', 'pray', 'immortal', 'imp'],
      },
      format: 'html',
    });
    expect(page.chips()).toEqual(['tell', 'newbie', 'pray', 'immortal', 'imp']);
    expect(page.count()).toBe('3 of 5 lines');
    expect(page.rows().filter((r) => r.out)).toEqual([
      { text: '<1020hp 800m 930mv> ', why: 'prompt', out: true },
      {
        text: "Tolliver tells you '[Exits: north east south west]'",
        why: 'tell',
        out: true,
      },
    ]);
    expect(page.status()).toBe('Saves Thickening Woods, October 3.html to Downloads');
  });

  it('opens on the selected session newest log when nothing picked one', async () => {
    await draw(null);
    expect(named('logs_list_sessions')).toContainEqual({ limit: 1, scope: { thisSession: true } });
    expect((named('scene_preview')[0]?.range as { log: number }).log).toBe(7);
  });

  it('keeps your prompt once you turn Prompts on', async () => {
    const page = await draw(7);
    await page.act(() =>
      props<{ onChange: (e: unknown) => void }>(page.switches()[0]).onChange({
        target: { checked: true },
      }),
    );
    expect((named('scene_preview').at(-1)?.filter as { prompts: boolean }).prompts).toBe(true);
    expect(page.count()).toBe('4 of 5 lines');
  });

  it('starts on a line you click and ends on one you Shift click', async () => {
    const page = await draw(7);
    await page.act(() =>
      props<{ onClick: (e: unknown) => void }>(page.times()[1]).onClick({ shiftKey: false }),
    );
    expect(named('scene_preview').at(-1)?.range).toMatchObject({
      fromMs: new Date(2026, 9, 3, 21, 14).getTime(),
      fromId: 102,
    });
    await page.act(() =>
      props<{ onClick: (e: unknown) => void }>(page.times()[3]).onClick({ shiftKey: true }),
    );
    expect(named('scene_preview').at(-1)?.range).toMatchObject({
      fromId: 102,
      toMs: new Date(2026, 9, 3, 21, 15, 59, 999).getTime(),
      toId: 104,
    });
  });

  it('keeps a channel you take out of the list', async () => {
    const page = await draw(7);
    await page.act(() => props<{ onClick: () => void }>(page.labelled('Keep tell')).onClick());
    expect(page.chips()).toEqual(['newbie', 'pray', 'immortal', 'imp']);
  });

  it('saves the HTML file with the theme in front and says where it went', async () => {
    const page = await draw(7);
    await page.act(
      () => props<{ onClick: () => void }>(page.button('Save scene')).onClick(),
      false,
    );
    const saved = named('scene_save')[0] as {
      format: string;
      palette: { muted: string; ansi: string[] };
    };
    expect(saved.format).toBe('html');
    expect(saved.palette.muted).toBe('#646260');
    expect(saved.palette.ansi).toHaveLength(16);
    expect(page.status()).toBe('Saved Thickening Woods, October 3.html in Downloads');
  });

  it('says why a save failed and goes back to the log view on Cancel', async () => {
    const page = await draw(7);
    failSave = true;
    await page.act(
      () => props<{ onClick: () => void }>(page.button('Save scene')).onClick(),
      false,
    );
    expect(page.errors.at(-1)).toBe(
      'Error: Vosh could not save the scene in your Downloads folder.',
    );
    await page.act(() => props<{ onClick: () => void }>(page.button('Cancel')).onClick(), false);
    expect(page.moves).toEqual([{ group: 'logs', section: 'search' }]);
  });

  it('goes back to the Logs tab on Cancel when no log was picked', async () => {
    const page = await draw(null);
    await page.act(() => props<{ onClick: () => void }>(page.button('Cancel')).onClick(), false);
    expect(page.moves).toEqual([{ group: 'logs' }]);
  });
});
