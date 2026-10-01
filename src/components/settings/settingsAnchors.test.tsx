import { act, createElement, type ComponentType } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import golden from '../../../fixtures/links/settings-anchors.json';
import { HELP_GOTO_EVENT } from '../../lib/helpLink';
import { resolveHelpTarget } from '../../lib/helpNav';
import { buildPaletteEntries, type PaletteDeps } from '../../lib/palette';
import { defaultLayout } from '../../lib/paneLayout';
import { normalizeUiConfig, type UiConfig } from '../../lib/session';
import {
  formatSettingsTarget,
  resolveSettingsTarget,
  settingsScrollIds,
  settingsSubpage,
  SETTINGS_GROUPS,
  type SettingsGroup,
  type SettingsTarget,
} from '../../lib/settingsNav';
import { SETTINGS_ROWS, settingsRowKey } from '../../lib/settingsSearch';
import { FakeDocument, findAll, type FakeElement } from '../../test/fakeDom';
import type { SettingsPageProps } from './pageTypes';

// Every way into Settings names a target as a string: a search hit, a
// palette row (whose id also sits in the palette's Recent list), an old
// tab id from an older build, and a link from another window or page.
// The frame resolves the string, opens the group's page, and scrolls to
// the element that carries the target's anchor as data-st-anchor. A
// move that drops or renames one of those anchors, or changes where a
// string lands, sends you to the top of a page with no error anywhere.
// So does a book button whose help topic id goes away.
//
// The golden file pins every link string and where it lands, the
// anchors each page draws, and the help topics the pages link to. This
// test mounts each page on every link into it, the way the frame does,
// and checks it draws the anchors the frame scrolls to.

// What the pages read when they mount. Two profiles, so a link that
// names one is told apart from the profile in use, and a tick, so the
// Timers list draws its pinned Tick row.
const PROFILES = {
  active: 'default',
  profiles: [
    { name: 'default' },
    { name: 'erelei', auto_match: { host: 'play.theforsakenlands.com', port: 1848 } },
  ],
};

const TICK = {
  enabled: true,
  interval_secs: 60,
  auto_fire: null,
  sound: false,
  reset_pattern: null,
  warn_at_secs: null,
  warn_message: null,
  warn_color: null,
};

const calls = vi.hoisted(() => ({
  invoked: [] as { cmd: string; args: Record<string, unknown> | undefined }[],
  emitted: [] as { event: string; payload: unknown }[],
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: Record<string, unknown>) => {
    calls.invoked.push({ cmd, args });
    return Promise.resolve(answer(cmd, args));
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn((event: string, payload: unknown) => {
    calls.emitted.push({ event, payload });
    return Promise.resolve();
  }),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
// CodeMirror needs a real DOM. The JSON view carries its anchor on the
// section around the editor, so nothing takes the editor's place.
vi.mock('@uiw/react-codemirror', () => ({ default: () => null }));

function answer(cmd: string, args: Record<string, unknown> | undefined): unknown {
  switch (cmd) {
    case 'profiles_list':
      return PROFILES;
    case 'profile_detail_get': {
      const name = String(args?.name);
      return {
        name,
        display_name: name === 'default' ? 'Default' : name,
        active: name === PROFILES.active,
        auto_match: PROFILES.profiles.find((p) => p.name === name)?.auto_match ?? null,
        world_name: null,
        tracked_affects: [],
        panes: defaultLayout(),
        generation: null,
        login_on: false,
      };
    }
    case 'tick_get_config':
      return TICK;
    case 'logs_list_sessions':
      return [];
    case 'logs_search_page':
      return { hits: [], total: 0 };
    default:
      return undefined;
  }
}

// ── Mounting a page ─────────────────────────────────────────────────

/** The page each group opens. Keep it in step with PAGES in
 *  SettingsApp.tsx. */
let PAGES: Record<SettingsGroup, ComponentType<SettingsPageProps>>;
let createRoot: typeof import('react-dom/client').createRoot;
const doc = new FakeDocument();

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
  // React DOM reads the user agent, and xterm, which the prompt preview
  // loads, reads the platform. Node 20 has no navigator of its own.
  vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
  const store = new Map<string, string>();
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, String(value)),
    removeItem: (key: string) => void store.delete(key),
  });
  class NoObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
    takeRecords() {
      return [];
    }
  }
  vi.stubGlobal('MutationObserver', NoObserver);
  vi.stubGlobal('ResizeObserver', NoObserver);
  vi.stubGlobal('requestAnimationFrame', (cb: () => void) => setTimeout(cb, 0));
  vi.stubGlobal('cancelAnimationFrame', (id: number) => clearTimeout(id));
  vi.stubGlobal('getComputedStyle', () => ({ getPropertyValue: () => '' }));
  // React DOM checks for a DOM once, when it loads, so it and the pages
  // load now.
  ({ createRoot } = await import('react-dom/client'));
  const [general, appearance, layout, input, automation, characters] = await Promise.all([
    import('./groups/GeneralGroup'),
    import('./pages/AppearancePage'),
    import('./groups/LayoutGroup'),
    import('./groups/InputGroup'),
    import('./pages/AutomationPage'),
    import('./pages/CharactersPage'),
  ]);
  PAGES = {
    general: general.GeneralGroup,
    appearance: appearance.AppearancePage,
    layout: layout.LayoutGroup,
    input: input.InputGroup,
    automation: automation.AutomationPage,
    characters: characters.CharactersPage,
  };
  await mountEveryLink();
}, 60_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

function config(): UiConfig {
  return normalizeUiConfig({
    theme: 'nord',
    auto_update: false,
    font_family: 'Menlo',
    font_size: 14,
    tracked_affects: [],
    enabled_presets: [],
  });
}

/** Where a link is offered: on macOS or off it, in loadout mode or
 *  not. A link to a row the search index offers only somewhere opens
 *  there, and every other link opens on macOS outside loadout mode. */
interface Env {
  mac: boolean;
  pathB: boolean;
}

function envFor(target: SettingsTarget): Env {
  const key = formatSettingsTarget(target);
  const only = SETTINGS_ROWS.find((row) => settingsRowKey(row) === key)?.only;
  return { mac: only !== 'not-macos', pathB: only === 'path-b' };
}

/** What a page drew for one link. */
interface Landing {
  anchors: string[];
  /** The pressed segments, the Automation kind among them. */
  pressed: string[];
  /** The profile the Characters page read, or null. */
  profile: string | null;
  /** The topics the book buttons open. */
  help: string[];
}

/** Call an element's click handler. This DOM sends no events, so read
 *  the handler from the props React keeps on the element. */
function press(el: FakeElement) {
  const key = Object.keys(el).find((k) => k.startsWith('__reactProps$'));
  const props = key
    ? (el as unknown as Record<string, { onClick?: (e: unknown) => void }>)[key]
    : undefined;
  if (!props?.onClick) throw new Error('the element has no click handler');
  props.onClick({ preventDefault() {}, stopPropagation() {} });
}

async function settle() {
  for (let i = 0; i < 10; i++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

async function land(target: SettingsTarget, env: Env): Promise<Landing> {
  doc.documentElement.dataset.platform = env.mac ? 'macos' : 'windows';
  calls.invoked.length = 0;
  calls.emitted.length = 0;
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const props: SettingsPageProps = {
    target,
    navSeq: 0,
    config: config(),
    setConfig: () => undefined,
    onError: () => undefined,
    pathB: env.pathB,
    navigate: () => undefined,
    setLeaveGuard: () => undefined,
  };
  await act(async () => {
    root.render(createElement(PAGES[target.group], props));
  });
  await settle();

  const anchors = findAll(container, (el) => el.hasAttribute('data-st-anchor')).map(
    (el) => el.getAttribute('data-st-anchor') ?? '',
  );
  const pressed = findAll(container, (el) => el.getAttribute('aria-pressed') === 'true').map(
    (el) => el.textContent,
  );
  const details = calls.invoked.filter((c) => c.cmd === 'profile_detail_get');
  const profile = details.length > 0 ? String(details[details.length - 1].args?.name) : null;
  const books = findAll(container, (el) =>
    (el.getAttribute('aria-label') ?? '').startsWith('Help on '),
  );
  for (const book of books) press(book);
  const help = calls.emitted
    .filter((e) => e.event === HELP_GOTO_EVENT)
    .map((e) => String(e.payload));

  await act(async () => {
    root.unmount();
  });
  doc.body.removeChild(container);
  return { anchors, pressed, profile, help };
}

// ── Every link, once ────────────────────────────────────────────────

/** Each link string the golden file names, from every group of them,
 *  with where it lands. */
const LINKS: Readonly<Record<string, string>> = Object.assign({}, ...Object.values(golden.links));

/** What each page drew, by the link string it opened on. */
const landings = new Map<string, Landing>();

/** The link each palette row that opens Settings sends, by the row's
 *  id, which is also the id the palette keeps in Recent. */
let palette: Record<string, string> = {};

/** Run every palette row and keep the ones that open Settings. */
async function paletteLinks(): Promise<Record<string, string>> {
  const sent: string[] = [];
  const deps: PaletteDeps = {
    connected: false,
    paneTypes: [],
    paneVisible: () => false,
    togglePane: () => undefined,
    openHelp: () => undefined,
    openFind: () => undefined,
    // The main window opens Settings where it was for the plain row.
    openSettings: () => undefined,
    openSettingsTab: (tab) => void sent.push(tab),
    connect: () => undefined,
    disconnect: () => undefined,
    insertInput: () => undefined,
  };
  const opened: Record<string, string> = {};
  for (const entry of buildPaletteEntries(deps)) {
    const before = sent.length;
    await entry.run();
    if (sent.length > before) opened[entry.id] = sent[sent.length - 1];
  }
  return opened;
}

/** Every link string, as the code sends it today and as the golden
 *  file names it, the strings older builds left behind and where they
 *  land, and each group bare, so the anchors a page draws on its own
 *  count too. */
function everyLink(): string[] {
  return [
    ...SETTINGS_GROUPS.map(({ id }) => id),
    ...SETTINGS_ROWS.map(settingsRowKey),
    ...golden.search,
    ...Object.keys(LINKS),
    ...Object.values(LINKS),
  ];
}

async function mountOnce(key: string) {
  if (landings.has(key)) return;
  const target = resolveSettingsTarget(key);
  landings.set(key, await land(target, envFor(target)));
}

async function mountEveryLink() {
  for (const key of everyLink()) await mountOnce(key);
  // The palette rows run after the pages draw, so a row that changes a
  // store cannot change what a page draws.
  palette = await paletteLinks();
  for (const link of Object.values(palette)) await mountOnce(link);
}

// ── The tests ───────────────────────────────────────────────────────

describe('Settings links', () => {
  it('search lists the same rows at the same links', () => {
    expect(SETTINGS_ROWS.map(settingsRowKey)).toEqual(golden.search);
  });

  it('palette rows open the same links', () => {
    expect(palette).toEqual(golden.palette);
  });

  it('every link string lands where it did', () => {
    for (const group of Object.values(golden.links)) {
      for (const [from, to] of Object.entries(group)) {
        expect(formatSettingsTarget(resolveSettingsTarget(from)), from).toBe(to);
      }
    }
    // Each palette row's link is one of them.
    for (const link of Object.values(golden.palette)) {
      expect(Object.keys(LINKS), link).toContain(link);
    }
  });

  it('every link opens a page that draws what the frame scrolls to', () => {
    for (const [key, landing] of landings) {
      const target = resolveSettingsTarget(key);
      for (const id of settingsScrollIds(target)) {
        expect(landing.anchors, `${key} draws ${id}`).toContain(id);
      }
      // A section the frame does not scroll to names an Automation
      // kind, a profile, or a page inside the group.
      if (target.group === 'automation' && target.section) {
        expect(
          landing.pressed.map((p) => p.toLowerCase()),
          key,
        ).toContain(target.section);
      }
      if (target.group === 'characters') {
        // No section means the profile in use.
        const wanted = target.section ?? PROFILES.active;
        expect(landing.profile?.toLowerCase(), key).toBe(wanted.toLowerCase());
      }
      if (settingsSubpage(target) !== null) {
        // The page inside the group takes the place of the group page.
        const own = landings.get(target.group)?.anchors ?? [];
        expect(own.length, key).toBeGreaterThan(0);
        for (const anchor of own) expect(landing.anchors, key).not.toContain(anchor);
      }
    }
  });

  it('each page draws the same anchors as before', () => {
    const drawn: Record<string, Set<string>> = {};
    for (const [key, landing] of landings) {
      const group = resolveSettingsTarget(key).group;
      drawn[group] ??= new Set();
      for (const anchor of landing.anchors) drawn[group].add(anchor);
    }
    const sorted = Object.fromEntries(
      SETTINGS_GROUPS.map(({ id }) => [id, [...(drawn[id] ?? [])].sort()]),
    );
    expect(sorted).toEqual(golden.anchors);
  });

  it('every book button opens a help topic that exists', () => {
    const topics = new Set<string>();
    for (const landing of landings.values()) for (const t of landing.help) topics.add(t);
    expect([...topics].sort()).toEqual(golden.help);
    for (const topic of golden.help) {
      expect(resolveHelpTarget(topic)?.kind, topic).toBe('topic');
    }
  });
});
