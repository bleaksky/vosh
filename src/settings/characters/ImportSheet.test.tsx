import { act, createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { ImportPreview, ImportResult } from '../../ipc/characters';
import type { ProfileEntry } from '../../ipc/profiles';
import { FakeDocument, findAll, type FakeElement } from '../../test/fakeDom';
import { ImportSheet } from './ImportSheet';
import { LUA_WARNING, type ImportFile } from './profileImport';

// The import sheet of board 5, drawn as markup for what it shows and
// mounted for Add as and Import.

const calls = vi.hoisted(() => ({
  invoked: [] as { cmd: string; args: unknown }[],
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: { name?: string }) => {
    calls.invoked.push({ cmd, args });
    const done: ImportResult = {
      name: args?.name ?? '',
      moved_from: [],
      kept_with: [],
      catalog_group: null,
      clashes: [],
    };
    return Promise.resolve(done);
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const WORLD = 'play.theforsakenlands.com';

const PREVIEW: ImportPreview = {
  name: 'Healer',
  triggers: 3,
  aliases: 2,
  macros: 2,
  timers: 1,
  tick: true,
  variables: 2,
  panes: ['map', 'chat', 'group'],
  runs_lua: [
    { kind: 'trigger', name: 'tells' },
    { kind: 'alias', name: 'heal' },
  ],
  plugins: ['vitals_alert'],
  world: { host: WORLD, port: 1848, name: 'The Forsaken Lands' },
  characters: [{ name: 'Orla', claimed_by: 'Healer' }],
  presets_stay: false,
};

// Board 5's list, Default and Healer on The Forsaken Lands.
const PROFILES: ProfileEntry[] = [
  { name: 'default', auto_match: { host: WORLD, port: 1848, characters: [] } },
  { name: 'Healer', auto_match: { host: WORLD, port: 1848, characters: ['Orla'] } },
];

const file = (preview: Partial<ImportPreview> = {}): ImportFile => ({
  fileName: 'Healer profile.toml',
  text: '[vosh_export]',
  preview: { ...PREVIEW, ...preview },
});

const none = () => undefined;

function draw(f: ImportFile, profiles: ProfileEntry[] = PROFILES): string {
  return renderToStaticMarkup(
    <ImportSheet
      file={f}
      profiles={profiles}
      fallback="default"
      onImported={none}
      onCancel={none}
      onError={none}
    />,
  );
}

/** The primary button, its label and whether it is off. */
function primary(html: string) {
  const [tag, label] = /<button[^>]*btn is-primary[^>]*>([^<]*)</.exec(html) ?? [];
  return { label, off: tag?.includes('disabled=""') ?? null };
}

describe('the import sheet', () => {
  it('heads the sheet with the file and starts the name from it', () => {
    const html = draw(file());
    expect(html).toMatch(/<h2[^>]*>Import Healer profile.toml<\/h2>/);
    expect(html).toMatch(/>Cancel<\/button>/);
    expect(html).toMatch(/aria-pressed="true"[^>]*>New profile</);
    expect(html).toMatch(/<input[^>]*class="st-field"[^>]*value="Healer"/);
  });

  it('says the name is taken in the danger tone and holds Import off', () => {
    const html = draw(file());
    expect(html).toContain(
      'class="st-row-desc" data-tone="danger">You already have a profile named Healer.</span>',
    );
    expect(html).toMatch(/<input[^>]*aria-invalid="true"/);
    expect(primary(html)).toEqual({ label: 'Import', off: true });

    // A name no profile has in any case is free.
    const free = draw(file({ name: 'Healer 2' }));
    expect(free).not.toContain('data-tone="danger"');
    expect(free).not.toContain('aria-invalid');
    expect(primary(free)).toEqual({ label: 'Import', off: false });

    // A file name that gives no name leaves the field for you.
    const blank = draw(file({ name: null }));
    expect(blank).toMatch(/<input[^>]*value=""/);
    expect(primary(blank)).toEqual({ label: 'Import', off: true });
  });

  it('shows the Lua warning, the summary and the claimed character off with its note', () => {
    const html = draw(file());
    expect(html).toContain(LUA_WARNING);
    expect(html).toContain(
      '<dt class="st-import-key">Runs Lua</dt><dd class="st-import-value">Trigger <span class="st-auto-mono">tells</span>, alias <span class="st-auto-mono">heal</span></dd>',
    );
    expect(html).toMatch(/<h2[^>]*>The Forsaken Lands<\/h2>/);
    expect(html).toContain('Use this profile when you log in as Orla');
    expect(html).not.toMatch(/role="switch"[^>]*checked=""/);
    expect(html).toContain(
      'Healer uses Orla now. Turn this on to move Orla here, and Healer&#x27;s login turns off.',
    );
  });

  it('leaves out the warning for a file with no Lua, and the world for one with no character', () => {
    const html = draw(file({ runs_lua: [], characters: [] }));
    expect(html).not.toContain(LUA_WARNING);
    expect(html).not.toContain('The Forsaken Lands');
  });
});

describe('Add as and Import', () => {
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

  /** The props React keeps on `el`, since this DOM sends no events. */
  function props<T>(el: FakeElement): T {
    const key = Object.keys(el).find((k) => k.startsWith('__reactProps$')) ?? '';
    return (el as unknown as Record<string, T>)[key];
  }

  function button(root: FakeElement, label: string): FakeElement {
    return findAll(root, (el) => el.nodeName === 'BUTTON' && el.textContent === label)[0];
  }

  /** Mount the sheet over `f`, run each of `steps` on it in turn, and
   *  say what it drew and what it imported. */
  async function mount(f: ImportFile, ...steps: ((root: FakeElement) => void)[]) {
    calls.invoked.length = 0;
    const imported: string[] = [];
    const container = doc.createElement('div');
    doc.body.appendChild(container);
    const root = createRoot(container as unknown as HTMLElement);
    await act(async () => {
      root.render(
        createElement(ImportSheet, {
          file: f,
          profiles: PROFILES,
          fallback: 'default',
          onImported: (_result, sentence) => void imported.push(sentence),
          onCancel: none,
          onError: none,
        }),
      );
    });
    for (const step of steps) {
      await act(async () => {
        step(container);
      });
    }
    await act(async () => {
      await Promise.resolve();
    });
    const drawn = {
      text: container.textContent,
      fields: findAll(
        container,
        (el) => el.nodeName === 'INPUT' && el.getAttribute('class') === 'st-field',
      ).length,
      // React picks a select's value through its options.
      picked: findAll(
        container,
        (el) =>
          el.nodeName === 'OPTION' && (el as unknown as { selected?: boolean }).selected === true,
      ).map((el) => el.textContent),
    };
    await act(async () => {
      root.unmount();
    });
    doc.body.removeChild(container);
    return { drawn, imported };
  }

  const press = (label: string) => (root: FakeElement) =>
    props<{ onClick: () => void }>(button(root, label)).onClick();

  it('swaps a select of your profiles in for the name under Replace, and drops the world', async () => {
    const { drawn, imported } = await mount(file(), press('Replace a profile'));
    expect(drawn.fields).toBe(0);
    // Replace starts on the profile that has the file's name.
    expect(drawn.picked).toEqual(['Healer']);
    expect(drawn.text).not.toContain('The Forsaken Lands');
    expect(drawn.text).not.toContain('You already have a profile named Healer.');
    expect(drawn.text).toContain('CancelReplace');
    expect(imported).toEqual([]);
  });

  it('replaces the profile you picked and passes no character', async () => {
    const { imported } = await mount(
      file(),
      press('Replace a profile'),
      (root) => {
        const select = findAll(root, (el) => el.nodeName === 'SELECT')[0];
        props<{ onChange: (e: unknown) => void }>(select).onChange({
          target: { value: 'default' },
        });
      },
      press('Replace'),
    );
    expect(calls.invoked).toEqual([
      {
        cmd: 'profile_import_apply',
        args: {
          fileName: 'Healer profile.toml',
          text: '[vosh_export]',
          addAs: 'replace',
          name: 'default',
          logins: [],
        },
      },
    ]);
    expect(imported).toEqual([
      'Vosh replaced Default with Healer profile.toml. Default keeps its world and characters.',
    ]);
  });

  it('imports a new profile with each character you left on', async () => {
    const f = file({
      name: 'Healer 2',
      characters: [
        { name: 'Orla', claimed_by: 'Healer' },
        { name: 'Maren', claimed_by: null },
      ],
    });
    const { imported } = await mount(f, press('Import'));
    // Maren, whom no profile has, starts on, and Orla starts off.
    expect(calls.invoked).toEqual([
      {
        cmd: 'profile_import_apply',
        args: {
          fileName: 'Healer profile.toml',
          text: '[vosh_export]',
          addAs: 'new',
          name: 'Healer 2',
          logins: ['Maren'],
        },
      },
    ]);
    expect(imported).toEqual(['Vosh added Healer 2 from Healer profile.toml.']);
  });

  it('moves a claimed character you turn on, and leaves out one you turn off', async () => {
    const f = file({
      name: 'Healer 2',
      characters: [
        { name: 'Orla', claimed_by: 'Healer' },
        { name: 'Maren', claimed_by: null },
      ],
    });
    const flip = (row: number, checked: boolean) => (root: FakeElement) => {
      const toggle = findAll(root, (el) => el.getAttribute('role') === 'switch')[row];
      props<{ onChange: (e: unknown) => void }>(toggle).onChange({ target: { checked } });
    };
    await mount(f, flip(0, true), flip(1, false), press('Import'));
    expect(calls.invoked.map((c) => (c.args as { logins: string[] }).logins)).toEqual([['Orla']]);
  });
});
