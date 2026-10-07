import { act, createElement, type ReactNode } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { ImportPreview, ImportResult } from '../../ipc/characters';
import { defaultLayout } from '../../panel/paneLayout';
import { FakeDocument, findAll, type FakeElement } from '../../test/fakeDom';
import type { SettingsPageProps } from '../pageTypes';

// Characters with the import of board 5: Import… reads the file you
// pick, the sheet takes the detail column while no row reads selected,
// and an import selects the profile the file went to.

const WORLD = 'play.theforsakenlands.com';
const NOT_AN_EXPORT =
  'catalog.toml is not a Vosh profile export. Import other clients under Automation.';

const PREVIEW: ImportPreview = {
  name: 'Healer',
  triggers: 3,
  aliases: 2,
  macros: 2,
  timers: 1,
  tick: true,
  variables: 2,
  panes: ['map', 'chat', 'group'],
  runs_lua: [{ kind: 'trigger', name: 'tells' }],
  plugins: [],
  world: { host: WORLD, port: 1848, name: 'The Forsaken Lands' },
  characters: [{ name: 'Orla', claimed_by: 'Healer' }],
  presets_stay: false,
};

const state = vi.hoisted(() => ({
  invoked: [] as { cmd: string; args: Record<string, unknown> | undefined }[],
  imported: false,
}));

function answer(cmd: string, args: Record<string, unknown> | undefined): unknown {
  switch (cmd) {
    case 'profiles_list':
      return {
        active: 'default',
        profiles: [
          { name: 'default', auto_match: { host: WORLD, port: 1848, characters: [] } },
          { name: 'Healer', auto_match: { host: WORLD, port: 1848, characters: ['Orla'] } },
          ...(state.imported
            ? [
                {
                  name: 'Healer 2',
                  auto_match: { host: WORLD, port: 1848, characters: [], enabled: false },
                },
              ]
            : []),
        ],
      };
    case 'profile_detail_get': {
      const name = String(args?.name);
      return {
        name,
        display_name: name === 'default' ? 'Default' : name,
        active: name === 'default',
        auto_match: null,
        world_name: 'The Forsaken Lands',
        tracked_affects: [],
        panes: defaultLayout(),
        generation: null,
        login_on: false,
      };
    }
    case 'profile_import_read':
      if (args?.fileName === 'catalog.toml') throw NOT_AN_EXPORT;
      return PREVIEW;
    case 'profile_import_apply': {
      state.imported = true;
      const done: ImportResult = {
        name: String(args?.name),
        moved_from: [],
        kept_with: [{ character: 'Orla', profile: 'Healer' }],
        catalog_group: null,
        clashes: [],
      };
      return done;
    }
    default:
      return null;
  }
}

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: Record<string, unknown>) => {
    state.invoked.push({ cmd, args });
    try {
      return Promise.resolve(answer(cmd, args));
    } catch (e) {
      return Promise.reject(e);
    }
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
vi.mock('../../ui/MenuSurface', async (actual) => ({
  ...(await actual<typeof import('../../ui/MenuSurface')>()),
  MenuSurface: ({ label, children }: { label: string; children: ReactNode }) =>
    createElement('menu', { 'aria-label': label }, children),
}));

const doc = new FakeDocument();
let createRoot: typeof import('react-dom/client').createRoot;
let CharactersPage: typeof import('./CharactersPage').CharactersPage;

beforeAll(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.stubGlobal('document', doc);
  vi.stubGlobal('window', {
    document: doc,
    location: { protocol: 'about:' },
    HTMLIFrameElement: class {},
    addEventListener() {},
    removeEventListener() {},
    matchMedia: () => ({ matches: true, addEventListener() {}, removeEventListener() {} }),
    setTimeout: globalThis.setTimeout.bind(globalThis),
    clearTimeout: globalThis.clearTimeout.bind(globalThis),
  });
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  vi.stubGlobal('localStorage', {
    getItem: () => null,
    setItem: () => undefined,
    removeItem: () => undefined,
  });
  ({ createRoot } = await import('react-dom/client'));
  ({ CharactersPage } = await import('./CharactersPage'));
});

afterAll(() => {
  vi.unstubAllGlobals();
});

async function settle() {
  for (let i = 0; i < 10; i++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

/** The props React keeps on `el`, since this DOM sends no events. */
function props<T>(el: FakeElement): T {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
  return (el as unknown as Record<string, T>)[key];
}

/** What the page shows: the selected rows, the detail headings and the
 *  line under the list. */
function shown(root: FakeElement) {
  return {
    selected: findAll(root, (el) => el.getAttribute('aria-current') === 'true').map(
      (el) => el.getAttribute('data-profile') ?? '',
    ),
    headings: findAll(root, (el) => el.nodeName === 'H2').map((el) => el.textContent),
    status:
      findAll(root, (el) => el.getAttribute('class') === 'st-chars-status')[0]?.textContent ?? '',
  };
}

/** Pick `name` in the file input Import… opens. */
function pick(root: FakeElement, name: string) {
  const input = findAll(
    root,
    (el) => el.nodeName === 'INPUT' && el.getAttribute('type') === 'file',
  )[0];
  const file = { name, text: () => Promise.resolve('[vosh_export]') };
  props<{ onChange: (e: unknown) => void }>(input).onChange({
    target: { files: [file], value: name },
  });
}

function press(root: FakeElement, label: string) {
  const button = findAll(root, (el) => el.nodeName === 'BUTTON' && el.textContent === label)[0];
  props<{ onClick: () => void }>(button).onClick();
}

/** Mount Characters on Healer and run `steps` in turn, saying what the
 *  page shows after each. */
async function run(...steps: ((root: FakeElement) => void)[]) {
  state.imported = false;
  state.invoked.length = 0;
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const page: SettingsPageProps = {
    target: { group: 'characters', section: 'Healer' },
    navSeq: 0,
    config: null,
    setConfig: () => undefined,
    onError: () => undefined,
    pathB: false,
    navigate: () => undefined,
    setLeaveGuard: () => undefined,
  };
  await act(async () => {
    root.render(createElement(CharactersPage, page));
  });
  await settle();
  const seen = [shown(container)];
  for (const step of steps) {
    await act(async () => {
      step(container);
    });
    await settle();
    seen.push(shown(container));
  }
  await act(async () => {
    root.unmount();
  });
  doc.body.removeChild(container);
  return seen;
}

describe('Import under Characters', () => {
  it('shows the sheet with no row selected, and Cancel brings the profile back', async () => {
    const [before, sheet, cancelled] = await run(
      (root) => pick(root, 'Healer profile.toml'),
      (root) => press(root, 'Cancel'),
    );
    expect(before.selected).toEqual(['Healer']);
    expect(sheet.selected).toEqual([]);
    expect(sheet.headings).toEqual([
      'Import Healer profile.toml',
      'In this file',
      'The Forsaken Lands',
    ]);
    expect(state.invoked).toContainEqual({
      cmd: 'profile_import_read',
      args: { fileName: 'Healer profile.toml', text: '[vosh_export]' },
    });
    expect(cancelled.selected).toEqual(['Healer']);
    expect(cancelled.headings[0]).toBe('Healer');
  });

  it('says a file that is no export on the line under the list', async () => {
    const [, refused] = await run((root) => pick(root, 'catalog.toml'));
    expect(refused.status).toBe(NOT_AN_EXPORT);
    expect(refused.selected).toEqual(['Healer']);
    expect(refused.headings[0]).toBe('Healer');
  });

  it('selects the profile the file went to and says what happened', async () => {
    const [, , , done] = await run(
      (root) => pick(root, 'Healer profile.toml'),
      (root) => {
        const field = findAll(
          root,
          (el) => el.nodeName === 'INPUT' && el.getAttribute('class') === 'st-field',
        );
        props<{ onChange: (e: unknown) => void }>(field[0]).onChange({
          target: { value: 'Healer 2' },
        });
      },
      (root) => press(root, 'Import'),
    );
    expect(done.selected).toEqual(['Healer 2']);
    expect(done.headings[0]).toBe('Healer 2');
    expect(done.status).toBe(
      'Vosh added Healer 2 from Healer profile.toml. Orla stays with Healer, so Healer 2 starts with its login off.',
    );
  });
});
