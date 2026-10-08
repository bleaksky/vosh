import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import aliasesExport from '../../../fixtures/ipc/aliases_export.json?raw';
import {
  buildAliasEntries,
  buildPaletteEntries,
  chooseTheme,
  initialSelection,
  paletteSections,
  themeEntries,
  themesInGalleryOrder,
  type PaletteDeps,
} from './palette';
import { appShortcut } from '../../lib/appMenu';
import { resolveSettingsTarget } from '../../lib/settingsNav';
import type { SessionRow } from '../../ipc/session';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTheme: () => Promise.resolve() }),
}));

function deps(over: Partial<PaletteDeps> = {}): PaletteDeps {
  return {
    connected: true,
    host: 'play.theforsakenlands.com',
    worldName: 'The Forsaken Lands',
    panelOpen: true,
    togglePanel: () => {},
    splitOpen: false,
    toggleSplit: () => {},
    paneTypes: ['map', 'affects', 'group', 'chat'],
    paneVisible: (pane) => pane === 'map' || pane === 'affects',
    togglePane: () => {},
    openHelp: () => {},
    openFind: () => {},
    openSettingsTab: () => {},
    connect: () => {},
    disconnect: () => {},
    insertInput: () => {},
    ...over,
  };
}

const flat = (sections: ReturnType<typeof paletteSections>) => sections.flatMap((s) => s.rows);

function sessionRow(id: number, character: string | null, port: number): SessionRow {
  return {
    id,
    name: null,
    character,
    host: 'play.theforsakenlands.com',
    port,
    tls: false,
    profile: 'Default',
    connected: true,
    since: null,
    selected: id === 1,
  };
}

/** Tolliver and Orla on the build port, Tolliver in front. */
function sessions() {
  return {
    rows: [sessionRow(1, 'Tolliver', 1848), sessionRow(2, 'Orla', 1825)],
    selected: 1,
    shown: true,
    goTo: vi.fn(),
    step: vi.fn(),
    toggleShown: vi.fn(),
  };
}

describe('paletteSections', () => {
  it('lists the approved View and Session rows with Disconnect last', () => {
    const sections = paletteSections(buildPaletteEntries(deps()), '', []);
    expect(sections.map((s) => s.label)).toEqual(['View', 'Session']);
    expect(sections[0].rows.map((r) => r.title)).toEqual([
      'Show panel',
      'Split terminal',
      'Choose theme',
    ]);
    expect(sections[0].rows[1].keys).toBe('Mod+\\');
    const rows = flat(sections);
    expect(rows[rows.length - 1].id).toBe('disconnect');
    expect(rows.filter((r) => r.destructive).map((r) => r.id)).toEqual(['disconnect']);
  });

  it('checks the toggles that are on', () => {
    const rows = buildPaletteEntries(deps());
    const checked = rows.filter((r) => r.checked).map((r) => r.id);
    expect(checked).toEqual(['panel', 'pane-map', 'pane-affects']);
    const split = buildPaletteEntries(deps({ splitOpen: true })).find((r) => r.id === 'split');
    expect(split?.checked).toBe(true);
  });

  it('lists a Show row for each pane type the shell offers', () => {
    const ids = (paneTypes: PaletteDeps['paneTypes']) =>
      buildPaletteEntries(deps({ paneTypes }))
        .filter((r) => r.id.startsWith('pane-'))
        .map((r) => r.id);
    expect(ids(['map', 'affects', 'group', 'chat'])).not.toContain('pane-imm');
    expect(ids(['map', 'imm'])).toEqual(['pane-map', 'pane-imm']);
  });

  it('finds Get started in View once you type, beside Open help', () => {
    const openGetStarted = vi.fn();
    const entries = buildPaletteEntries(deps({ openGetStarted }));
    expect(flat(paletteSections(entries, '', [])).map((r) => r.id)).not.toContain('get-started');
    const view = paletteSections(entries, 'get started', []).find((s) => s.label === 'View');
    expect(view?.rows.map((r) => r.id)).toContain('get-started');
    for (const word of ['walkthrough', 'welcome', 'suggestions', 'presets']) {
      expect(flat(paletteSections(entries, word, [])).map((r) => r.id)).toContain('get-started');
    }
    const ids = entries.map((r) => r.id);
    expect(ids.indexOf('get-started')).toBe(ids.indexOf('help') + 1);
    void entries.find((r) => r.id === 'get-started')?.run();
    expect(openGetStarted).toHaveBeenCalled();
    expect(buildPaletteEntries(deps()).some((r) => r.id === 'get-started')).toBe(false);
  });

  it('picks where your prompt shows only while the profile reads one', async () => {
    const none = buildPaletteEntries(deps());
    expect(none.some((r) => r.id.startsWith('prompt-show-'))).toBe(false);
    const rows = buildPaletteEntries(deps({ promptShow: 'lifted' })).filter((r) =>
      r.id.startsWith('prompt-show-'),
    );
    expect(rows.map((r) => r.title)).toEqual([
      'Show your prompt in the text',
      'Lift your prompts in the text',
      'Pin your prompt above the command line',
    ]);
    expect(rows.map((r) => r.checked)).toEqual([false, true, false]);
    expect(rows.every((r) => r.section === 'view' && r.searchOnly)).toBe(true);
    // Typing finds them, and the list you open on stays as it was.
    const found = flat(
      paletteSections(buildPaletteEntries(deps({ promptShow: 'text' })), 'pin', []),
    );
    expect(found.map((r) => r.id)).toContain('prompt-show-pinned');
    const opened = paletteSections(buildPaletteEntries(deps({ promptShow: 'text' })), '', []);
    expect(flat(opened).some((r) => r.id.startsWith('prompt-show-'))).toBe(false);
    vi.mocked(invoke).mockClear();
    await rows[2].run();
    expect(invoke).toHaveBeenCalledWith('session_send_input', { line: '#prompt show pinned' });
  });

  it('finds the prompt card in an Input section of its own as you type prompt', () => {
    const opened: (string | null)[] = [];
    const drawn: boolean[] = [];
    const entries = buildPaletteEntries(
      deps({
        openPromptCard: (view) => opened.push(view ?? null),
        promptDraw: true,
        setPromptDraw: (on) => drawn.push(on),
      }),
    );
    const input = entries.filter((r) => r.section === 'input');
    expect(input.map((r) => [r.title, r.checked ?? null])).toEqual([
      ['Customize prompt…', null],
      ['Draw your prompt', true],
      ['Edit prompt as text…', null],
    ]);
    expect(input.every((r) => r.searchOnly)).toBe(true);
    const typed = paletteSections(entries, 'prompt', []);
    expect(typed[0].label).toBe('Input');
    expect(typed[0].rows.map((r) => r.title)).toEqual([
      'Customize prompt…',
      'Draw your prompt',
      'Edit prompt as text…',
    ]);
    // The list you open on stays as it was.
    expect(paletteSections(entries, '', []).map((s) => s.label)).toEqual(['View', 'Session']);
    void input[0].run();
    void input[1].run();
    void input[2].run();
    expect(opened).toEqual([null, 'text']);
    expect(drawn).toEqual([false]);
  });

  it('leaves Draw your prompt out while the profile reads no prompt', () => {
    const entries = buildPaletteEntries(deps({ openPromptCard: () => {}, promptDraw: null }));
    expect(entries.filter((r) => r.section === 'input').map((r) => r.title)).toEqual([
      'Customize prompt…',
      'Edit prompt as text…',
    ]);
    expect(buildPaletteEntries(deps()).some((r) => r.section === 'input')).toBe(false);
  });

  it('names the world on the connect row when you are offline', () => {
    const rows = buildPaletteEntries(deps({ connected: false }));
    const last = rows[rows.length - 1];
    expect(last.title).toBe('Connect to The Forsaken Lands');
    expect(last.destructive).toBeFalsy();
  });

  it('offers Connect then Disconnect while a redial waits', () => {
    const rows = buildPaletteEntries(deps({ connected: false, redialing: true }));
    expect(rows.slice(-2).map((r) => r.id)).toEqual(['connect', 'disconnect']);
  });

  it('leads with Recent and keeps destructive commands out of it', () => {
    const sections = paletteSections(buildPaletteEntries(deps()), '', [
      'disconnect',
      'find',
      'settings-themes',
      'gone',
      'profile-save',
      'help',
    ]);
    expect(sections[0].label).toBe('Recent');
    expect(sections[0].rows.map((r) => r.id)).toEqual(['find', 'settings-themes', 'profile-save']);
  });

  it('lists New session… with its key once you type, leading the Session rows', () => {
    const newSession = vi.fn();
    const entries = buildPaletteEntries(deps({ newSession }));
    expect(flat(paletteSections(entries, '', [])).map((r) => r.id)).not.toContain('session-new');
    const session = paletteSections(entries, 'session', []).find((s) => s.label === 'Session');
    const row = session?.rows[0];
    expect(row).toMatchObject({ id: 'session-new', title: 'New session…', keys: 'Mod+T' });
    void row?.run();
    expect(newSession).toHaveBeenCalled();
    expect(buildPaletteEntries(deps()).some((r) => r.id === 'session-new')).toBe(false);
  });

  it('lists Close session with its key once you type, after New session…', () => {
    const closeSession = vi.fn();
    const entries = buildPaletteEntries(deps({ newSession: vi.fn(), closeSession }));
    expect(flat(paletteSections(entries, '', [])).map((r) => r.id)).not.toContain('session-close');
    const session = paletteSections(entries, 'session', []).find((s) => s.label === 'Session');
    const ids = session?.rows.map((r) => r.id) ?? [];
    expect(ids.indexOf('session-close')).toBe(ids.indexOf('session-new') + 1);
    const row = session?.rows.find((r) => r.id === 'session-close');
    expect(row).toMatchObject({ title: 'Close session', keys: 'Mod+W' });
    expect(row?.destructive).toBeUndefined();
    void row?.run();
    expect(closeSession).toHaveBeenCalled();
    expect(buildPaletteEntries(deps()).some((r) => r.id === 'session-close')).toBe(false);
  });

  it('lists the session rows and every session to go to, as board 4 draws them', () => {
    const two = sessions();
    const entries = buildPaletteEntries(
      deps({ newSession: vi.fn(), renameSession: vi.fn(), closeSession: vi.fn(), sessions: two }),
    );
    // Nothing typed, the palette opens as before.
    expect(flat(paletteSections(entries, '', [])).map((r) => r.id)).toEqual([
      'panel',
      'split',
      'theme',
      'disconnect',
    ]);
    const found = paletteSections(entries, 'session', []);
    expect(found.map((s) => s.label)).toEqual(['View', 'Session', 'Go to']);
    const [, session, goTo] = found;
    expect(session.rows.map((r) => [r.title, r.keys])).toEqual([
      ['New session…', 'Mod+T'],
      ['Next session', 'Mod+Shift+]'],
      ['Previous session', 'Mod+Shift+['],
      ['Rename session…', undefined],
      ['Close session', 'Mod+W'],
      ['Hide sessions', appShortcut('sessions-sidebar')],
      ['Disconnect', undefined],
    ]);
    expect(goTo.rows.map((r) => [r.title, r.meta, r.keys, r.checked])).toEqual([
      ['Tolliver', 'The Forsaken Lands', 'Mod+1', true],
      ['Orla', 'The Forsaken Lands 1825', 'Mod+2', false],
    ]);
  });

  it('steps, goes to a session and hides the sidebar through the shell', () => {
    const two = sessions();
    const entries = buildPaletteEntries(deps({ sessions: two }));
    const run = (id: string) => void entries.find((e) => e.id === id)?.run();
    run('session-next');
    run('session-previous');
    expect(two.step.mock.calls).toEqual([[1], [-1]]);
    run('session-goto-2');
    expect(two.goTo).toHaveBeenCalledWith(2);
    run('sessions-sidebar');
    expect(two.toggleShown).toHaveBeenCalled();
    const hidden = buildPaletteEntries(deps({ sessions: { ...two, shown: false } }));
    expect(hidden.find((e) => e.id === 'sessions-sidebar')?.title).toBe('Show sessions');
    // The keycap reads the platform key, Ctrl Cmd S on macOS.
    expect(appShortcut('sessions-sidebar', true)).toBe('Ctrl+Mod+S');
    expect(appShortcut('sessions-sidebar', false)).toBe('Mod+Shift+S');
  });

  it('finds a session by its name or its world', () => {
    const entries = buildPaletteEntries(deps({ sessions: sessions() }));
    const titles = (q: string) =>
      paletteSections(entries, q, [])
        .find((s) => s.label === 'Go to')
        ?.rows.map((r) => r.title);
    expect(titles('orla')).toEqual(['Orla']);
    expect(titles('1825')).toEqual(['Orla']);
  });

  it('lists Rename session… once you type, and runs it through the shell', () => {
    const renameSession = vi.fn();
    const entries = buildPaletteEntries(deps({ renameSession }));
    expect(flat(paletteSections(entries, '', [])).map((r) => r.id)).not.toContain('session-rename');
    const row = paletteSections(entries, 'rename', [])
      .find((s) => s.label === 'Session')
      ?.rows.find((r) => r.id === 'session-rename');
    expect(row).toMatchObject({ title: 'Rename session…' });
    expect(row?.keys).toBeUndefined();
    void row?.run();
    expect(renameSession).toHaveBeenCalled();
    expect(buildPaletteEntries(deps()).some((r) => r.id === 'session-rename')).toBe(false);
  });

  it('leaves the rows between sessions out with one session', () => {
    const one = { ...sessions(), rows: [sessionRow(1, 'Tolliver', 1848)] };
    const entries = buildPaletteEntries(
      deps({ newSession: vi.fn(), renameSession: vi.fn(), closeSession: vi.fn(), sessions: one }),
    );
    const ids = entries.map((e) => e.id);
    expect(ids).toContain('session-new');
    // Rename session… shows whenever the shell passes it.
    expect(ids).toContain('session-rename');
    expect(ids).toContain('session-close');
    for (const id of ['session-next', 'session-previous', 'sessions-sidebar', 'session-goto-1']) {
      expect(ids).not.toContain(id);
    }
  });

  it('keys the first nine sessions and names one before its login by its world', () => {
    const rows = [
      sessionRow(1, 'Tolliver', 1848),
      ...Array.from({ length: 9 }, (_, i) => sessionRow(i + 2, null, 1825)),
    ];
    const entries = buildPaletteEntries(deps({ sessions: { ...sessions(), rows } }));
    const goTo = entries.filter((e) => e.section === 'goto');
    expect(goTo.map((e) => e.keys)).toEqual([
      'Mod+1',
      'Mod+2',
      'Mod+3',
      'Mod+4',
      'Mod+5',
      'Mod+6',
      'Mod+7',
      'Mod+8',
      'Mod+9',
      undefined,
    ]);
    expect(goTo[1]).toMatchObject({ title: 'The Forsaken Lands 1825' });
    expect(goTo[1].meta).toBeUndefined();
  });

  it('hides search only rows until you type, then ranks matches by section', () => {
    const entries = buildPaletteEntries(deps());
    expect(flat(paletteSections(entries, '', [])).some((r) => r.searchOnly)).toBe(false);
    const sections = paletteSections(entries, 'settings', ['find']);
    expect(sections.map((s) => s.label)).toEqual(['View']);
    expect(sections[0].rows[0].id).toBe('settings');
    expect(sections[0].rows.map((r) => r.id)).toContain('settings-themes');
  });
});

describe('snoop rows (SN8)', () => {
  const tab = (name: string, live: boolean) => ({
    name,
    live,
    ended_at: live ? null : 1_800_000_000_000,
    last_output_at: null,
  });
  const snoops = (tabs: ReturnType<typeof tab>[]) => ({
    tabs,
    goTo: vi.fn(),
    next: vi.fn(),
    stop: vi.fn(),
    openWindow: vi.fn(),
    closeEnded: vi.fn(),
  });
  const titles = (over: Partial<PaletteDeps>) =>
    flat(paletteSections(buildPaletteEntries(deps(over)), 'snoop', [])).map((r) => r.title);

  it('lists the SN8 rows, word for word, while a snoop is open', () => {
    const all = snoops([tab('Tolliver', true), tab('Maren', true), tab('Orla', false)]);
    expect(titles({ snoops: all })).toEqual([
      'Go to snoop',
      'Next snoop',
      'Stop snooping Tolliver',
      'Stop snooping Maren',
      'Stop every snoop',
      'Open snoop in a window',
      'Close ended snoops',
    ]);
  });

  it('offers none with no snoop open, the way staff queues waits', () => {
    expect(titles({})).toEqual([]);
    expect(titles({ snoops: snoops([]) })).toEqual([]);
  });

  it('offers each row only where it acts', () => {
    expect(titles({ snoops: snoops([tab('Tolliver', true)]) })).toEqual([
      'Go to snoop',
      'Stop snooping Tolliver',
      'Stop every snoop',
      'Open snoop in a window',
    ]);
    expect(titles({ snoops: snoops([tab('Orla', false)]) })).toEqual([
      'Go to snoop',
      'Open snoop in a window',
      'Close ended snoops',
    ]);
  });

  it('keeps them out of the palette until you type, with the key on Go to snoop', () => {
    const one = snoops([tab('Tolliver', true)]);
    const home = flat(paletteSections(buildPaletteEntries(deps({ snoops: one })), '', []));
    expect(home.some((r) => r.id.startsWith('snoop'))).toBe(false);
    const go = buildPaletteEntries(deps({ snoops: one })).find((r) => r.id === 'snoop');
    expect(go?.keys).toBe('Mod+J');
    expect(go?.section).toBe('session');
  });

  it('runs each row on its own call', async () => {
    const all = snoops([tab('Tolliver', true), tab('Maren', true), tab('Orla', false)]);
    const rows = buildPaletteEntries(deps({ snoops: all }));
    const run = (id: string) => rows.find((r) => r.id === id)?.run();
    await run('snoop');
    await run('snoop-next');
    await run('snoop-stop-Maren');
    await run('snoop-stop-all');
    await run('snoop-window');
    await run('snoop-close-ended');
    expect(all.goTo).toHaveBeenCalledTimes(1);
    expect(all.next).toHaveBeenCalledTimes(1);
    expect(all.stop.mock.calls).toEqual([['Maren'], []]);
    expect(all.openWindow).toHaveBeenCalledTimes(1);
    expect(all.closeEnded).toHaveBeenCalledTimes(1);
  });
});

describe('settings rows', () => {
  const settingsRows = (over: Partial<PaletteDeps> = {}) =>
    buildPaletteEntries(deps(over)).filter((r) => r.id.startsWith('settings-'));

  it('keeps the old ids so Recent rows survive, except vitals', () => {
    const ids = settingsRows().map((r) => r.id);
    for (const id of [
      'themes',
      'typography',
      'tick',
      'panels',
      'general',
      'profiles',
      'triggers',
      'aliases',
      'macros',
      'timers',
      'import',
      'logs',
    ]) {
      expect(ids).toContain(`settings-${id}`);
    }
    expect(ids).not.toContain('settings-vitals');
    // A Recent row for the removed vitals entry drops out quietly.
    const recent = paletteSections(buildPaletteEntries(deps()), '', [
      'settings-vitals',
      'settings-themes',
    ]);
    expect(recent[0].rows.map((r) => r.id)).toEqual(['settings-themes']);
  });

  it('names the places the rows open now', () => {
    const title = (id: string) => settingsRows().find((r) => r.id === `settings-${id}`)?.title;
    expect(title('themes')).toBe('Open theme settings');
    expect(title('typography')).toBe('Open terminal text settings');
    expect(title('panels')).toBe('Open panel layout settings');
    expect(title('profiles')).toBe('Open character settings');
    expect(title('input')).toBe('Open input settings');
    expect(title('import')).toBe('Import from another client…');
  });

  it('shows the Settings keys on the four Automation rows only', () => {
    const keyed = settingsRows()
      .filter((r) => r.keys)
      .map((r) => [r.id, r.keys]);
    expect(keyed).toEqual([
      ['settings-triggers', appShortcut('settings-triggers')],
      ['settings-aliases', appShortcut('settings-aliases')],
      ['settings-macros', appShortcut('settings-macros')],
      ['settings-timers', appShortcut('settings-timers')],
    ]);
    // Cmd+Option on macOS, since Cmd Shift 3 and 4 take screenshots
    // there, and Ctrl+Shift on Windows and Linux.
    expect(appShortcut('settings-timers', true)).toBe('Mod+Alt+1');
    expect(appShortcut('settings-aliases', true)).toBe('Mod+Alt+2');
    expect(appShortcut('settings-triggers', true)).toBe('Mod+Alt+3');
    expect(appShortcut('settings-macros', true)).toBe('Mod+Alt+4');
    expect(appShortcut('settings-timers', false)).toBe('Mod+Shift+1');
    expect(appShortcut('settings-aliases', false)).toBe('Mod+Shift+2');
    expect(appShortcut('settings-triggers', false)).toBe('Mod+Shift+3');
    expect(appShortcut('settings-macros', false)).toBe('Mod+Shift+4');
  });

  it('opens each row on a place the resolver knows', () => {
    const opened: string[] = [];
    for (const row of settingsRows({ openSettingsTab: (tab) => opened.push(tab) })) row.run();
    const groups = opened.map((tab) => resolveSettingsTarget(tab).group);
    expect(groups).toEqual([
      'appearance',
      'appearance',
      'automation',
      'characters',
      'general',
      'input',
      'characters',
      'automation',
      'automation',
      'automation',
      'automation',
      'automation',
      'general',
    ]);
    expect(resolveSettingsTarget(opened[0])).toEqual({ group: 'appearance', section: 'theme' });
    expect(resolveSettingsTarget(opened[opened.length - 1])).toEqual({
      group: 'general',
      section: 'logs',
    });
  });
});

describe('alias rows', () => {
  // The reply to aliases_export, which a Rust test in ipc/automation.rs
  // holds to the Alias serialization byte for byte. A disabled alias
  // stays in the list, and a Lua alias keeps the expansion it had before
  // Lua was turned on.
  const exported = aliasesExport.trimEnd();

  async function aliasRows(over: Partial<PaletteDeps> = {}) {
    vi.mocked(invoke).mockClear();
    vi.mocked(invoke).mockImplementationOnce(((command: string) =>
      Promise.resolve(command === 'aliases_export' ? exported : undefined)) as typeof invoke);
    return buildAliasEntries(deps(over));
  }

  const row = (rows: Awaited<ReturnType<typeof aliasRows>>, name: string) => {
    const found = rows.find((r) => r.title === name);
    if (!found) throw new Error(`no row for ${name}`);
    return found;
  };

  it('shows the command each alias sends', async () => {
    const rows = await aliasRows();
    // The palette names no profile, so the app reads the selected
    // session's.
    expect(invoke).toHaveBeenCalledWith('aliases_export', { profile: undefined });
    expect(rows.map((r) => [r.title, r.meta ?? null])).toEqual([
      ['cs', 'cast %1'],
      ['k', 'kill %1'],
      ['lk', 'mud.send("look")'],
      ['lt', 'mud.send("look " .. captures[1])'],
      ['rec', 'recall'],
    ]);
    // Typing part of the command finds the alias, and a Lua alias
    // answers to its script, never to the expansion it ignores.
    expect(flat(paletteSections(rows, 'kill', [])).map((r) => r.id)).toEqual(['alias-k']);
    expect(flat(paletteSections(rows, 'looked', [])).map((r) => r.id)).toEqual(['alias-lt']);
  });

  it('fills the command line for an alias that takes arguments', async () => {
    const inserted: string[] = [];
    const rows = await aliasRows({ insertInput: (text) => inserted.push(text) });
    void row(rows, 'k').run();
    void row(rows, 'lt').run();
    void row(rows, 'cs').run();
    expect(inserted).toEqual(['k ', 'lt ', 'cs ']);
    expect(invoke).not.toHaveBeenCalledWith('session_send_input', expect.anything());
    // A Lua alias that never reads its captures runs at once, whatever
    // its old expansion held.
    void row(rows, 'lk').run();
    void row(rows, 'rec').run();
    expect(inserted).toEqual(['k ', 'lt ', 'cs ']);
    expect(invoke).toHaveBeenCalledWith('session_send_input', { line: 'lk' });
    expect(invoke).toHaveBeenCalledWith('session_send_input', { line: 'rec' });
  });
});

describe('initialSelection', () => {
  it('never opens on a destructive row', () => {
    const entries = buildPaletteEntries(deps());
    expect(initialSelection(flat(paletteSections(entries, '', [])))).toBe(0);
    const onlyDisconnect = flat(paletteSections(entries, 'disconnect', []));
    expect(onlyDisconnect.map((r) => r.id)).toEqual(['disconnect']);
    expect(initialSelection(onlyDisconnect)).toBe(-1);
  });

  it('selects a destructive row you typed for', () => {
    const entries = buildPaletteEntries(deps());
    const onlyDisconnect = flat(paletteSections(entries, 'disconnect', []));
    expect(initialSelection(onlyDisconnect, 'disconnect')).toBe(0);
    expect(initialSelection([], 'nothing matches')).toBe(-1);
  });
});

describe('theme order', () => {
  afterEach(async () => {
    const { setCustomThemes } = await import('../../theme/themes');
    setCustomThemes([]);
  });

  it('lists the themes in the gallery order, your own themes last', async () => {
    const { customToAppTheme, setCustomThemes } = await import('../../theme/themes');
    setCustomThemes([
      customToAppTheme({
        id: 'mine',
        label: 'Nord (custom)',
        description: '',
        xterm: {},
        chrome: {},
      }),
    ]);
    const ordered = themesInGalleryOrder();
    expect(ordered.slice(0, 7).map((t) => t.theme.id)).toEqual([
      'triad',
      'rubric',
      'nord',
      'obsidian-ember',
      'gruvbox',
      'rose-pine',
      'tokyo-night',
    ]);
    const rest = ordered.slice(7, -1).map((t) => t.theme.label);
    expect(rest).toEqual([...rest].sort((a, b) => a.localeCompare(b)));
    // The menu bar's Choose theme lists the same order.
    expect(rest).toEqual([
      'Catppuccin',
      'Classic Vivid',
      'Dracula at Night',
      'Everforest Dark',
      'Green Screen',
      'Harbor Dark',
      'High Contrast',
      'Iceberg Dark',
      'Kanso Zen',
      'Melange Dark',
      'Melange Light',
      'Modus Vivendi',
      'Monokai',
      'Nightfly',
      'One Half Dark',
      'Solarized Dark',
      'Solarized Light',
      'Srcery',
      'Tango Dark',
    ]);
    expect(ordered.at(-1)).toMatchObject({ theme: { id: 'mine' }, custom: true });
    expect(ordered.filter((t) => t.custom)).toHaveLength(1);
    expect(themeEntries().map((e) => e.id)).toEqual(ordered.map((t) => `theme-${t.theme.id}`));
  });
});

describe('chooseTheme', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
    // A dark OS.
    vi.stubGlobal('window', {
      matchMedia: (query: string) => ({
        matches: query.includes('dark'),
        addEventListener: () => {},
        removeEventListener: () => {},
      }),
    });
    vi.stubGlobal('document', {
      documentElement: { setAttribute: () => {}, style: { setProperty: () => {} } },
    });
  });

  afterEach(async () => {
    const { applyThemePrefs } = await import('../../theme/theme');
    applyThemePrefs({
      theme: 'obsidian-ember',
      follow_system_appearance: false,
      light_theme: 'rubric',
      dark_theme: 'obsidian-ember',
      theme_follow: 'off',
      day_theme: '',
      night_theme: '',
    });
    vi.unstubAllGlobals();
  });

  it('sets the manual pick while follow is off', async () => {
    const { applyThemePrefs, getCurrentThemeId, getThemePrefs } = await import('../../theme/theme');
    applyThemePrefs({
      theme: 'nord',
      follow_system_appearance: false,
      light_theme: 'rubric',
      dark_theme: 'nord',
      theme_follow: 'off',
      day_theme: '',
      night_theme: '',
    });
    await chooseTheme('gruvbox');
    expect(getCurrentThemeId()).toBe('gruvbox');
    expect(getThemePrefs()?.theme).toBe('gruvbox');
    expect(invoke).toHaveBeenCalledWith('ui_set_theme', {
      theme: 'gruvbox',
      lightTheme: 'rubric',
      darkTheme: 'nord',
      dayTheme: '',
      nightTheme: '',
    });
  });

  it('fills the slot that matches the pick while follow is on', async () => {
    const { applyThemePrefs, getCurrentThemeId, getThemePrefs } = await import('../../theme/theme');
    applyThemePrefs({
      theme: 'nord',
      follow_system_appearance: true,
      light_theme: 'rubric',
      dark_theme: 'tokyo-night',
      theme_follow: 'off',
      day_theme: '',
      night_theme: '',
    });
    await chooseTheme('rose-pine');
    expect(getCurrentThemeId()).toBe('rose-pine');
    expect(getThemePrefs()).toMatchObject({ theme: 'nord', dark_theme: 'rose-pine' });

    // A light pick fills the light slot and stays hidden on a dark OS.
    const { customToAppTheme, setCustomThemes } = await import('../../theme/themes');
    setCustomThemes([
      customToAppTheme({
        id: 'paper',
        label: 'Paper',
        description: '',
        xterm: { background: '#ffffff', foreground: '#222222' },
        chrome: {},
      }),
    ]);
    await chooseTheme('paper');
    setCustomThemes([]);
    expect(getThemePrefs()).toMatchObject({ light_theme: 'paper', dark_theme: 'rose-pine' });
    expect(getCurrentThemeId()).toBe('rose-pine');
    expect(invoke).toHaveBeenLastCalledWith('ui_set_theme', {
      theme: 'nord',
      lightTheme: 'paper',
      darkTheme: 'rose-pine',
      dayTheme: '',
      nightTheme: '',
    });
  });
});

describe('the theme on screen', () => {
  // A paint an older build left names Vellum, which this build shows
  // as Rubric.
  const oldPaint = JSON.stringify({
    v: 1,
    follow: false,
    manual: { id: 'vellum', appearance: 'light', vars: { '--bg': '#f7f4ee' } },
  });

  // Last in the file, since the fresh modules it loads leave the ones
  // the tests above import behind.
  it('checks the theme a retired id shows, from the startup paint on', async () => {
    vi.resetModules();
    const { prepaintTheme } = await import('../../theme/themePaint');
    prepaintTheme({
      storage: () => ({ getItem: () => oldPaint, setItem: () => {} }),
      systemDark: () => false,
      root: () => ({ setAttribute: () => {}, style: { setProperty: () => {} } }),
    });
    const { getCurrentThemeId } = await import('../../theme/theme');
    expect(getCurrentThemeId()).toBe('vellum');
    const palette = await import('./palette');
    const checked = palette.themeEntries().filter((e) => e.checked);
    expect(checked.map((e) => e.id)).toEqual(['theme-rubric']);
  });
});

describe('the writing card’s rows', () => {
  it('offers a row for each kind, named for what you do, found as you type', () => {
    const opened: string[] = [];
    const rows = buildPaletteEntries(
      deps({
        writing: {
          kinds: ['note', 'journal', 'application', 'idea', 'bug', 'typo'],
          beast: false,
          open: (kind) => opened.push(kind),
        },
      }),
    );
    const writing = rows.filter((row) => row.id.startsWith('write-'));
    expect(writing.map((row) => row.title)).toEqual([
      'Write a note…',
      'Write a journal entry…',
      'Write an application…',
      'Write an idea…',
      'Report a bug…',
      'Report a typo…',
      'Edit your description…',
      'Edit your history…',
      'Edit your personality…',
      'Edit your purpose…',
    ]);
    expect(writing.every((row) => row.section === 'input' && row.searchOnly)).toBe(true);
    writing.find((row) => row.title === 'Report a bug…')?.run();
    expect(opened).toEqual(['bug']);
  });

  it('adds the beast for a werebeast', () => {
    const rows = buildPaletteEntries(
      deps({ writing: { kinds: ['note'], beast: true, open: () => {} } }),
    );
    expect(rows.some((row) => row.title === 'Edit your beast description…')).toBe(true);
  });
});
