import { act, createElement, type ComponentType, type ReactNode } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import golden from '../../fixtures/links/settings-anchors.json';
import { presetById, presetTriggers } from '../automation/presets';
import { resolveHelpTarget } from '../help/helpNav';
import { buildPaletteEntries, type PaletteDeps } from '../shell/overlays/palette';
import { defaultLayout, type PaneLeaf } from '../panel/paneLayout';
import type { GameBlock } from '../prompt/promptSettings';
import { type SessionIdentity } from '../ipc/characters';
import { HELP_GOTO, SETTINGS_GOTO_TAB } from '../ipc/events';
import { type PromptLastSeen, type PromptState } from '../ipc/prompt';
import { normalizeUiConfig, type UiConfig } from '../ipc/uiConfig';
import {
  formatSettingsTarget,
  resolveSettingsTarget,
  settingsScrollIds,
  settingsSubpage,
  SETTINGS_GROUPS,
  type SettingsGroup,
  type SettingsTarget,
} from '../lib/settingsNav';
import { SETTINGS_ROWS, settingsRowKey } from './settingsSearch';
import { FakeDocument, FakeElement, findAll } from '../test/fakeDom';
import type { PaneMenu as PaneMenuType } from '../panel/PaneMenu';
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
// strings the pane menu and the links on other pages send, the anchors
// each page draws, and the help topics the pages link to. This test
// mounts each page on every link into it, the way the frame does, and
// checks it draws the anchors the frame scrolls to. It mounts each page
// twice: cold, as a Settings window opens on the link, and warm, as a
// window that already shows the group follows the link, since the frame
// keeps the page and hands it the new target. The Prompt section draws
// your game's prompt one of four ways, and a search hit on it lands in
// each, so the Input page mounts on that link in all four.

// What the pages read when they mount. Two profiles, so a link that
// names one is told apart from the profile in use, and a tick, so the
// Timers list draws its pinned Tick row.
const PROFILES = {
  active: 'default',
  profiles: [
    { name: 'default' },
    { name: 'ilsabet', auto_match: { host: 'play.theforsakenlands.com', port: 1848 } },
  ],
};

/** A preset trigger as the store keeps it. */
const SECONDARY = presetTriggers(presetById('disarm_buff_fade')!).find(
  (t) => t.name === 'disarm.secondary',
);

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

/** What the Prompt section reads: the profile's prompt table, the
 *  prompt state, where Vosh last saw your codes, and who is connected.
 *  What a scene leaves out reads as nothing. */
interface PromptScene {
  /** The way the section should draw your game's prompt. */
  block: GameBlock;
  config?: unknown;
  state?: PromptState;
  seen?: PromptLastSeen;
  identity?: SessionIdentity;
}

const FORSAKEN: SessionIdentity = {
  host: 'play.theforsakenlands.com',
  port: 1848,
  character: null,
  profile: 'default',
  claimed_by: null,
};

function forsakenState(newBuild: boolean): PromptState {
  return {
    catalog: [],
    status: { status: 'no_capture', last_match_at: null },
    new_build: newBuild,
    forsaken: true,
    open_row: null,
    packages: [],
  };
}

/** The link a search hit on Your game's prompt sends. */
const PROMPT_LINK = 'input:prompt#prompt-game';

const NO_PROMPT: PromptScene = { block: 'point' };

/** Each way the Prompt section draws your game's prompt, with what it
 *  reads to draw it that way. The pattern reads a prompt, so the
 *  preview draws too. */
const PROMPT_SCENES: Readonly<Record<string, PromptScene>> = {
  'The Forsaken Lands, once the game sends your codes': {
    block: 'codes',
    identity: FORSAKEN,
    state: forsakenState(true),
    seen: {
      prompt: '%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c',
      fprompt: '',
      enabled: true,
      at: null,
      at_login: true,
      source: 'gmcp',
      character: null,
    },
  },
  'The Forsaken Lands, before the game sends your codes': {
    block: 'fields',
    identity: FORSAKEN,
    state: forsakenState(false),
  },
  'another game, reading the line you pointed at': {
    block: 'line',
    config: { capture: { kind: 'regex', lines: ['^<(\\d+)hp>$'], settle: false } },
  },
  'another game, before you point at its line': NO_PROMPT,
};

/** What the mocks answer for the page being drawn: the profile in use,
 *  or null while the list fails to read, and the Prompt section's
 *  state. */
const scene: { active: string | null; prompt: PromptScene } = {
  active: PROFILES.active,
  prompt: NO_PROMPT,
};

const calls = vi.hoisted(() => ({
  invoked: [] as { cmd: string; args: Record<string, unknown> | undefined }[],
  emitted: [] as { event: string; payload: unknown }[],
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string, args?: Record<string, unknown>) => {
    calls.invoked.push({ cmd, args });
    try {
      return Promise.resolve(answer(cmd, args));
    } catch (e) {
      return Promise.reject(e);
    }
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
// A menu measures itself and portals to the page. The pane menu draws
// its rows in place here instead. The Settings pages open a menu only
// on a press, so they draw as they do in the app.
vi.mock('../ui/MenuSurface', async (actual) => ({
  ...(await actual<typeof import('../ui/MenuSurface')>()),
  MenuSurface: ({ label, children }: { label: string; children: ReactNode }) =>
    createElement('menu', { 'aria-label': label }, children),
}));

function answer(cmd: string, args: Record<string, unknown> | undefined): unknown {
  switch (cmd) {
    case 'profiles_list':
      if (scene.active === null) throw new Error('profiles_list failed');
      return { ...PROFILES, active: scene.active };
    case 'profile_detail_get': {
      const name = String(args?.name);
      return {
        name,
        display_name: name === 'default' ? 'Default' : name,
        active: name === scene.active,
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
    // The Presets page reads the list of presets that are on, and the
    // alert presets, to draw a row for each.
    case 'ui_get_config':
      return config();
    case 'alert_presets_get':
      return { ids: ['alert_tells'], on: [], alerts: {} };
    // The Triggers page lists a preset trigger, which a preset fix
    // notice opens by name.
    case 'triggers_export':
      return JSON.stringify([SECONDARY]);
    case 'preset_edits_get':
      return {};
    case 'logs_list_sessions':
      return [];
    case 'logs_search_page':
      return { hits: [], total: 0 };
    case 'prompt_config_get':
      return scene.prompt.config ?? null;
    case 'prompt_state_get':
      return scene.prompt.state ?? null;
    case 'prompt_last_seen':
      return scene.prompt.seen ?? null;
    case 'session_identity_get':
      return scene.prompt.identity ?? null;
    // The Macros page reads macros_list through the macro list store,
    // to ring a key your macro keeps from a preset.
    case 'macros_list':
    case 'plugins_list':
    case 'lua_output_get':
      return [];
    default:
      return undefined;
  }
}

// ── Mounting a page ─────────────────────────────────────────────────

/** The page each group opens. Keep it in step with PAGES in
 *  SettingsWindow.tsx. */
let PAGES: Record<SettingsGroup, ComponentType<SettingsPageProps>>;
let PaneMenu: typeof PaneMenuType;
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
    // The pane menu hands the caret back to the command line.
    dispatchEvent: () => true,
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
  // A link to a preset scrolls its row into view in the list. A color
  // field reads no color, as it reads none where CSS is missing.
  vi.stubGlobal('CSS', { escape: (s: string) => s, supports: () => false });
  Object.assign(FakeElement.prototype, { querySelector: () => null });
  // React DOM checks for a DOM once, when it loads, so it and the pages
  // load now.
  ({ createRoot } = await import('react-dom/client'));
  const [general, appearance, layout, input, automation, scripts, characters, paneMenu] =
    await Promise.all([
      import('./general/GeneralPage'),
      import('./appearance/AppearancePage'),
      import('./layout/LayoutPage'),
      import('./input/InputPage'),
      import('./automation/AutomationPage'),
      import('./scripts/ScriptsPage'),
      import('./characters/CharactersPage'),
      import('../panel/PaneMenu'),
    ]);
  PAGES = {
    general: general.GeneralPage,
    appearance: appearance.AppearancePage,
    layout: layout.LayoutPage,
    input: input.InputPage,
    automation: automation.AutomationPage,
    scripts: scripts.ScriptsPage,
    characters: characters.CharactersPage,
  };
  PaneMenu = paneMenu.PaneMenu;
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

const MAC: Env = { mac: true, pathB: false };

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
  /** The links the page sent through navigate, as strings. */
  sent: string[];
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

/** The one element under `root` that `match` finds. */
function only(root: FakeElement, what: string, match: (el: FakeElement) => boolean): FakeElement {
  const found = findAll(root, match);
  if (found.length !== 1) throw new Error(`found ${found.length} of ${what}`);
  return found[0];
}

async function settle() {
  for (let i = 0; i < 10; i++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

interface LandOptions {
  /** Draw the group bare first, then hand the same page the link, the
   *  way a window that already shows the group follows a search hit, a
   *  palette row, or a link. */
  warm?: boolean;
  /** Run once the page draws, to press something on it. */
  after?: (container: FakeElement) => void;
}

async function land(
  target: SettingsTarget,
  env: Env,
  { warm = false, after }: LandOptions = {},
): Promise<Landing> {
  doc.documentElement.dataset.platform = env.mac ? 'macos' : 'windows';
  calls.invoked.length = 0;
  calls.emitted.length = 0;
  const sent: string[] = [];
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  // The frame hands the page the same config and callbacks on every
  // navigation.
  const props: Omit<SettingsPageProps, 'target' | 'navSeq'> = {
    config: config(),
    setConfig: () => undefined,
    onError: () => undefined,
    pathB: env.pathB,
    navigate: (to) => void sent.push(formatSettingsTarget(to)),
    setLeaveGuard: () => undefined,
  };
  const draw = async (to: SettingsTarget, navSeq: number) => {
    await act(async () => {
      root.render(createElement(PAGES[target.group], { ...props, target: to, navSeq }));
    });
    await settle();
  };
  if (warm) await draw({ group: target.group }, 0);
  await draw(target, warm ? 1 : 0);

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
  const help = calls.emitted.filter((e) => e.event === HELP_GOTO).map((e) => String(e.payload));
  if (after) {
    await act(async () => {
      after(container);
    });
  }

  await act(async () => {
    root.unmount();
  });
  doc.body.removeChild(container);
  return { anchors, pressed, profile, help, sent };
}

// ── Every link, cold and warm ───────────────────────────────────────

/** Each link string the golden file names, from every group of them,
 *  with where it lands. */
const LINKS: Readonly<Record<string, string>> = Object.assign({}, ...Object.values(golden.links));

/** One page drawn for one link: cold, warm, or in a Prompt scene. */
interface Visit {
  link: string;
  how: string;
  landing: Landing;
}

const visits: Visit[] = [];

/** What each page drew cold, by the link string it opened on. */
const cold = new Map<string, Landing>();
const warmed = new Set<string>();

/** The link each palette row that opens Settings sends, by the row's
 *  id, which is also the id the palette keeps in Recent. */
let palette: Record<string, string> = {};

/** The links the pane menu and the links on other pages send, by
 *  sender. */
let senders: Record<string, string[]> = {};

/** The way the Prompt section drew your game's prompt in each scene. */
const blocks = new Map<string, GameBlock | null>();

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

/** Open a page on `key` cold, and warm unless it names only the group. */
async function mountOnce(key: string) {
  const target = resolveSettingsTarget(key);
  const env = envFor(target);
  if (!cold.has(key)) {
    const landing = await land(target, env);
    cold.set(key, landing);
    visits.push({ link: key, how: 'cold', landing });
  }
  if (!warmed.has(key) && formatSettingsTarget(target) !== target.group) {
    warmed.add(key);
    visits.push({ link: key, how: 'warm', landing: await land(target, env, { warm: true }) });
  }
}

/** Open the pane menu on an Affects pane, press the item whose text
 *  starts with `label`, and return the links it sends Settings. */
async function paneMenuLinks(label: string): Promise<string[]> {
  calls.emitted.length = 0;
  const container = doc.createElement('div');
  doc.body.appendChild(container);
  const root = createRoot(container as unknown as HTMLElement);
  const leaf: PaneLeaf = { id: 'leaf-affects', pane: 'affects', weight: 1, props: {} };
  const anchor = {
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 0, bottom: 0 }),
    closest: () => null,
  } as unknown as HTMLButtonElement;
  await act(async () => {
    root.render(createElement(PaneMenu, { leaf, anchor, onClose: () => undefined }));
  });
  await settle();
  const item = only(
    container,
    label,
    (el) => el.getAttribute('role') === 'menuitem' && el.textContent.startsWith(label),
  );
  await act(async () => {
    press(item);
  });
  await act(async () => {
    root.unmount();
  });
  doc.body.removeChild(container);
  return calls.emitted.filter((e) => e.event === SETTINGS_GOTO_TAB).map((e) => String(e.payload));
}

/** Press each link outside search and the palette: first with the
 *  profile in use named Ilsabet, then while the profile list fails to
 *  read, so no profile is known. */
async function senderLinks(): Promise<Record<string, string[]>> {
  const out: Record<string, string[]> = {
    'pane menu, Edit tracked affects': [],
    'pane menu, Change when affects warn': [],
    'Layout, Panes and tracked affects': [],
    'General, Search logs': [],
  };
  for (const active of ['Ilsabet', null]) {
    scene.active = active;
    out['pane menu, Edit tracked affects'].push(...(await paneMenuLinks('Edit tracked affects')));
    out['pane menu, Change when affects warn'].push(
      ...(await paneMenuLinks('Change when affects warn')),
    );
    const layout = await land({ group: 'layout' }, MAC, {
      after: (c) =>
        press(only(c, 'the panes row', (el) => el.getAttribute('data-st-anchor') === 'panes')),
    });
    out['Layout, Panes and tracked affects'].push(...layout.sent);
  }
  scene.active = PROFILES.active;
  const general = await land({ group: 'general' }, MAC, {
    after: (c) =>
      press(
        only(
          c,
          'Search logs',
          (el) => el.nodeName === 'BUTTON' && el.textContent === 'Search logs…',
        ),
      ),
  });
  out['General, Search logs'].push(...general.sent);
  return out;
}

/** The way the Prompt section drew your game's prompt, told by its
 *  markup and not by its anchor. */
function gameBlockDrawn(root: FakeElement): GameBlock | null {
  const classes = (el: FakeElement) => (el.getAttribute('class') ?? '').split(' ');
  const has = (match: (el: FakeElement) => boolean) => findAll(root, match).length > 0;
  if (has((el) => classes(el).includes('st-prompt-code'))) return 'codes';
  if (has((el) => el.nodeName === 'INPUT' && el.getAttribute('aria-label') === 'Prompt')) {
    return 'fields';
  }
  if (has((el) => classes(el).includes('st-prompt-line-row'))) return 'line';
  if (has((el) => classes(el).includes('st-prompt-point'))) return 'point';
  return null;
}

/** Open the Input page on the search hit for Your game's prompt in each
 *  scene. */
async function promptLinks() {
  const target = resolveSettingsTarget(PROMPT_LINK);
  for (const [name, prompt] of Object.entries(PROMPT_SCENES)) {
    scene.prompt = prompt;
    let drawn: GameBlock | null = null;
    const landing = await land(target, envFor(target), {
      after: (c) => {
        drawn = gameBlockDrawn(c);
      },
    });
    blocks.set(name, drawn);
    visits.push({ link: PROMPT_LINK, how: name, landing });
  }
  scene.prompt = NO_PROMPT;
}

async function mountEveryLink() {
  for (const key of everyLink()) await mountOnce(key);
  senders = await senderLinks();
  await promptLinks();
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

  it('the pane menu and links on other pages send the same links', () => {
    expect(senders).toEqual(golden.senders);
    // They are the links from other pages and windows, every one.
    const sent = [...new Set(Object.values(senders).flat())].sort();
    expect(sent).toEqual(Object.keys(golden.links['from other pages and windows']).sort());
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
    for (const { link, how, landing } of visits) {
      const at = `${link}, ${how}`;
      const target = resolveSettingsTarget(link);
      for (const id of settingsScrollIds(target)) {
        expect(landing.anchors, `${at} draws ${id}`).toContain(id);
      }
      // A section the frame does not scroll to names an Automation
      // kind, a profile, or a page inside the group.
      if (target.group === 'automation' && target.section) {
        expect(
          landing.pressed.map((p) => p.toLowerCase()),
          at,
        ).toContain(target.section);
      }
      if (target.group === 'characters') {
        // No section means the profile in use.
        const wanted = target.section ?? PROFILES.active;
        expect(landing.profile?.toLowerCase(), at).toBe(wanted.toLowerCase());
      }
      if (settingsSubpage(target) !== null) {
        // The page inside the group takes the place of the group page.
        const own = cold.get(target.group)?.anchors ?? [];
        expect(own.length, at).toBeGreaterThan(0);
        for (const anchor of own) expect(landing.anchors, at).not.toContain(anchor);
      }
    }
  });

  it('the Prompt section draws your game prompt where search lands, every way', () => {
    for (const [name, prompt] of Object.entries(PROMPT_SCENES)) {
      expect(blocks.get(name), name).toBe(prompt.block);
    }
  });

  it('each page draws the same anchors as before', () => {
    const drawn: Record<string, Set<string>> = {};
    for (const { link, landing } of visits) {
      const group = resolveSettingsTarget(link).group;
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
    for (const { landing } of visits) for (const t of landing.help) topics.add(t);
    expect([...topics].sort()).toEqual(golden.help);
    for (const topic of golden.help) {
      expect(resolveHelpTarget(topic)?.kind, topic).toBe('topic');
    }
  });
});
