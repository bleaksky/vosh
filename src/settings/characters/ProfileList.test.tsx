import { act, createElement, type ReactNode } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import type { ProfilesList } from '../../ipc/profiles';
import { FakeDocument, findAll, type FakeElement } from '../../test/fakeDom';
import { ProfileList } from './ProfileList';

// Export to Downloads from a profile's more menu. A profile with
// characters on its world asks which ones the file names, all off to
// start, and one with none exports at once.

const calls = vi.hoisted(() => ({
  invoked: [] as { cmd: string; args: unknown }[],
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: { name?: string }) => {
    // The sessions store loads its list once the page mounts, which no
    // export asks for.
    if (cmd !== 'sessions_list') calls.invoked.push({ cmd, args });
    return Promise.resolve(
      cmd === 'profile_export_file'
        ? { path: '/Downloads/x', file_name: `${args?.name ?? ''} profile.toml` }
        : null,
    );
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
// The menu draws its rows in place, and a confirm draws its words, its
// fields and its buttons with no focus trap, which needs more of the DOM
// than src/test/fakeDom.ts holds.
vi.mock('../../ui/MenuSurface', async (actual) => ({
  ...(await actual<typeof import('../../ui/MenuSurface')>()),
  MenuSurface: ({ label, children }: { label: string; children: ReactNode }) =>
    createElement('menu', { 'aria-label': label }, children),
}));
vi.mock('../../ui/ConfirmDialog', () => ({
  ConfirmDialog: (props: {
    title: string;
    body: string;
    confirmLabel: string;
    tone?: string;
    children?: ReactNode;
    onConfirm: () => void;
    onCancel: () => void;
  }) =>
    createElement(
      'div',
      { role: 'dialog', 'aria-label': props.title, 'data-tone': props.tone ?? 'danger' },
      createElement('p', null, props.body),
      props.children,
      createElement('button', { type: 'button', onClick: props.onCancel }, 'Cancel'),
      createElement('button', { type: 'button', onClick: props.onConfirm }, props.confirmLabel),
    ),
}));

const WORLD = 'play.theforsakenlands.com';

// The list: Default on The Forsaken Lands with no character, and
// Healer with Orla. Maren names a character but has no world.
const LIST: ProfilesList = {
  active: 'default',
  profiles: [
    { name: 'default', auto_match: { host: WORLD, port: 1848, characters: [] } },
    { name: 'Healer', auto_match: { host: WORLD, port: 1848, characters: ['Orla'] } },
    { name: 'Maren', auto_match: { characters: ['Maren'] } },
  ],
};

const none = () => undefined;
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

function button(root: FakeElement, match: (el: FakeElement) => boolean): FakeElement {
  return findAll(root, (el) => el.nodeName === 'BUTTON' && match(el))[0];
}

/** The more button of `profile`, pressed as from the pointer. */
const openMenu = (profile: string) => (root: FakeElement) => {
  const more = button(root, (el) => el.getAttribute('aria-label') === `${profile} options`);
  const anchor = {
    focus() {},
    getBoundingClientRect: () => ({ left: 0, right: 20, top: 0, bottom: 20 }),
  };
  props<{ onClick: (e: unknown) => void }>(more).onClick({ currentTarget: anchor });
};

const press = (label: string) => (root: FakeElement) =>
  props<{ onClick: () => void }>(button(root, (el) => el.textContent === label)).onClick();

/** Mount the list, run each of `steps` in turn, and say what showed. */
async function run(...steps: ((root: FakeElement) => void)[]) {
  calls.invoked.length = 0;
  const said: (string | null)[] = [];
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  await act(async () => {
    root.render(
      createElement(ProfileList, {
        list: LIST,
        selected: 'Healer',
        identity: null,
        status: null,
        onSelect: none,
        onShow: none,
        onStatus: (s) => void said.push(s),
        onError: none,
        onChanged: none,
        onImport: none,
      }),
    );
  });
  let dialog: { title: string | null; switches: (string | null)[] } | null = null;
  for (const step of steps) {
    await act(async () => {
      step(container);
    });
    const shown = findAll(container, (el) => el.getAttribute('role') === 'dialog')[0];
    if (shown) {
      dialog = {
        title: shown.getAttribute('aria-label'),
        switches: findAll(shown, (el) => el.getAttribute('role') === 'switch').map(
          (el) =>
            `${el.parentNode?.parentNode?.textContent ?? ''} ${String(props<{ checked: boolean }>(el).checked)}`,
        ),
      };
    }
  }
  await act(async () => {
    await Promise.resolve();
  });
  await act(async () => {
    root.unmount();
  });
  doc.body.removeChild(container);
  return { dialog, said };
}

describe('Export to Downloads', () => {
  it('asks which characters the file names, all off, for a profile that has some', async () => {
    const { dialog, said } = await run(openMenu('Healer'), press('Export to Downloads'));
    expect(dialog).toEqual({ title: 'Export Healer?', switches: ['Orla false'] });
    expect(calls.invoked).toEqual([]);
    expect(said).toEqual([]);
  });

  it('names no character unless you turn it on', async () => {
    await run(openMenu('Healer'), press('Export to Downloads'), press('Export'));
    expect(calls.invoked).toEqual([
      { cmd: 'profile_export_file', args: { name: 'Healer', characters: [] } },
    ]);

    const flip = (root: FakeElement) => {
      const toggle = findAll(root, (el) => el.getAttribute('role') === 'switch')[0];
      props<{ onChange: (e: unknown) => void }>(toggle).onChange({ target: { checked: true } });
    };
    const { said } = await run(
      openMenu('Healer'),
      press('Export to Downloads'),
      flip,
      press('Export'),
    );
    expect(calls.invoked).toEqual([
      { cmd: 'profile_export_file', args: { name: 'Healer', characters: ['Orla'] } },
    ]);
    expect(said).toEqual(['Vosh saved Healer profile.toml in your Downloads folder.']);
  });

  it('exports at once a profile with no character, or with no world', async () => {
    for (const name of ['default', 'Maren']) {
      const label = name === 'default' ? 'Default' : name;
      const { dialog } = await run(openMenu(label), press('Export to Downloads'));
      expect(dialog, name).toBeNull();
      expect(calls.invoked, name).toEqual([
        { cmd: 'profile_export_file', args: { name, characters: [] } },
      ]);
    }
  });
});
