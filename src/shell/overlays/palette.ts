import { resetPanelLayout } from '../../panel/panelReset';
import APP_SHORTCUTS from '../../lib/appShortcuts.json';
import { exportAliases } from '../../ipc/automation';
import { type PromptShow } from '../../ipc/prompt';
import type { WritingKind } from '../../ipc/writing';
import { KINDS } from '../../writing/kinds';
import { sendInput, type SessionRow } from '../../ipc/session';
import type { SnoopTab } from '../../ipc/snoop';
import { sessionLabel } from '../../lib/sessionLabel';
import { setUiTheme } from '../../ipc/uiConfig';
import type { PaneType } from '../../panel/paneLayout';
import {
  applyAndBroadcastTheme,
  applyThemePrefs,
  broadcastThemePrefs,
  getCurrentThemeId,
  getThemePrefs,
  pickTheme,
} from '../../theme/theme';
import { galleryThemes } from '../../theme/themeThumb';
import { BUILTIN_THEMES, THEMES, themeShownBy, type AppTheme } from '../../theme/themes';

// Command registry for the ⌘K palette. Commands are built fresh each
// time the palette opens so checks and labels reflect live state
// (connected, panel open, which panes show). Slash commands run
// through the same backend input pipeline as typed text, so the
// palette never grows a second command implementation.

// ── Registry ─────────────────────────────────────────────────────────

/** Home sections, in the order the palette lists them. With nothing
 *  typed the palette shows Recent, View, and Session (SPEC 7), so
 *  Disconnect is the final row. Aliases, settings, help, find and the
 *  sessions to go to surface as you type or through Recent. */
export type PaletteSection = 'input' | 'view' | 'aliases' | 'session' | 'goto';

/** Input holds the prompt card's rows, which show only as you type, so
 *  the palette still opens on View and Session. Go to follows Session,
 *  as board 4 of the Sessions review draws it. */
export const SECTION_ORDER: PaletteSection[] = ['input', 'view', 'aliases', 'session', 'goto'];

export const SECTION_LABELS: Record<PaletteSection | 'recent', string> = {
  recent: 'Recent',
  input: 'Input',
  view: 'View',
  aliases: 'Aliases',
  session: 'Session',
  goto: 'Go to',
};

export interface PaletteEntry {
  id: string;
  section: PaletteSection;
  title: string;
  /** Extra match terms beyond the title. */
  keywords?: string;
  /** Shortcut spec for the keycaps, like 'Mod+F'. Display only. */
  keys?: string;
  /** Toggle state. An on toggle shows a check mark. */
  checked?: boolean;
  /** Trailing detail in the tertiary tone (the current theme, an
   *  alias's command). */
  meta?: string;
  /** Set when `meta` is text that goes to the MUD, so it takes the
   *  terminal face. */
  metaMono?: boolean;
  /** Drawn in the danger color, kept out of Recent, and never the row
   *  the palette opens on. */
  destructive?: boolean;
  /** Listed only while you type. Recent can still surface it. */
  searchOnly?: boolean;
  /** A submenu. Picking the row opens this list in place, under the
   *  `childLabel` header. */
  children?: () => PaletteEntry[];
  childLabel?: string;
  run: () => void | Promise<void>;
}

/** The open sessions, for the rows that move between them. */
export interface PaletteSessions {
  /** Every open session, in the sidebar's order. */
  rows: readonly SessionRow[];
  selected: number;
  /** Whether you keep the sidebar showing them, though a narrow window
   *  can fold it. */
  shown: boolean;
  /** Bring a session to the front. */
  goTo: (session: number) => void;
  /** Bring the next session to the front, or the one before with -1. */
  step: (step: 1 | -1) => void;
  /** Hide the sidebar in this window, or show it again. */
  toggleShown: () => void;
}

/** The selected session's snoops, for the rows that reach them (Snoop
 *  SN8). */
export interface PaletteSnoops {
  /** Every tab, live or ended, in the order they started. */
  tabs: readonly SnoopTab[];
  /** Put the caret in the tab in front, as Cmd J does. */
  goTo: () => void;
  /** Step to the next tab and put the caret there. */
  next: () => void;
  /** Send `snoop stop` with `name`, or alone to stop every snoop. */
  stop: (name?: string) => void;
  /** Move the tabs into a window of their own, or bring it forward. */
  openWindow: () => void;
  /** Close every ended tab. */
  closeEnded: () => void;
}

export interface PaletteDeps {
  connected: boolean;
  /** A redial waits or dials after a drop, so Disconnect follows the
   *  Connect row to end it. */
  redialing?: boolean;
  /** Host of the live session, shown as the Session header chip. */
  host?: string | null;
  /** Display name of the world the connect row targets. */
  worldName?: string | null;
  /** Panel visibility. The Show panel row appears when the shell
   *  passes togglePanel. */
  panelOpen?: boolean;
  togglePanel?: () => void;
  /** Scrollback split state. The Split terminal row appears when the
   *  shell passes toggleSplit. */
  splitOpen?: boolean;
  toggleSplit?: () => void;
  /** Pane types that get a Show row, in order. */
  paneTypes: readonly PaneType[];
  paneVisible: (pane: PaneType) => boolean;
  togglePane: (pane: PaneType) => void;
  openHelp: () => void;
  /** Open Get started on its list. The row appears when the shell
   *  passes it. */
  openGetStarted?: () => void;
  openFind: () => void;
  /** Open Settings on its last tab. Falls back to the General tab. */
  openSettings?: () => void;
  openSettingsTab: (tab: string) => void;
  connect: () => void;
  /** Open a session on its New session form. The row appears when the
   *  shell passes it. */
  newSession?: () => void;
  /** Name the selected session, in its row or in the session popover.
   *  The row appears when the shell passes it. */
  renameSession?: (() => void) | undefined;
  /** Close the selected session, asking first while it is connected.
   *  The row appears when the shell passes it. */
  closeSession?: () => void;
  /** The open sessions. With two or more, Next session, Previous session,
   *  Hide sessions and a Go to row for each appear. */
  sessions?: PaletteSessions;
  /** The selected session's snoops. The snoop rows appear while it has
   *  one, the way Show staff queues waits for Imm.Queues. */
  snoops?: PaletteSnoops;
  disconnect: () => void;
  /** Put text into the input row and focus it (for parameterized
   *  aliases the user finishes typing). */
  insertInput: (text: string) => void;
  /** Where your prompt shows. The three rows that pick it appear only
   *  while the profile reads a prompt, so the shell passes it then. */
  promptShow?: PromptShow | null;
  /** Open the prompt card, or Edit as text with `text`. The Input rows
   *  appear when the shell passes it. */
  openPromptCard?: (view?: 'text') => void;
  /** Whether the profile draws its prompt, or null while it reads none,
   *  which leaves Draw your prompt out. */
  promptDraw?: boolean | null;
  setPromptDraw?: (on: boolean) => void;
  /** The writing card: the boards you write on, whether you have a beast
   *  to describe, and how to open it on a kind. The Input rows for each
   *  kind appear when the shell passes it. */
  writing?: { kinds: WritingKind[]; beast: boolean; open: (kind: WritingKind) => void };
}

const PROMPT_SHOW_ROWS: { show: PromptShow; title: string }[] = [
  { show: 'text', title: 'Show your prompt in the text' },
  { show: 'lifted', title: 'Lift your prompts in the text' },
  { show: 'pinned', title: 'Pin your prompt above the command line' },
];

const PANE_TITLES: Record<PaneType, string> = {
  map: 'Show map',
  affects: 'Show affects',
  group: 'Show group',
  chat: 'Show chat',
  imm: 'Show staff queues',
};

// Each id is a Settings deep link (src/lib/settingsNav.ts) and, as
// `settings-<id>`, a palette Recent id, so the old tab ids stay. The
// vitals row is gone because Settings no longer has vitals settings.
// Its id still resolves, to Layout.
const SETTINGS_TABS: { id: string; title: string; keywords: string }[] = [
  {
    id: 'themes',
    title: 'Open theme settings',
    keywords: 'appearance catalog editor terminal palette colors',
  },
  {
    id: 'typography',
    title: 'Open terminal text settings',
    keywords: 'appearance font face size typography',
  },
  { id: 'tick', title: 'Open tick settings', keywords: 'automation timer warn' },
  { id: 'panels', title: 'Open panel layout settings', keywords: 'characters panes layout' },
  { id: 'general', title: 'Open general settings', keywords: 'updates scope logs' },
  {
    id: 'input',
    title: 'Open input settings',
    keywords: 'command line caret cursor prompt spell check paste history',
  },
  {
    id: 'profiles',
    title: 'Open character settings',
    keywords: 'profiles characters hosts login tracked affects',
  },
  { id: 'triggers', title: 'Open trigger settings', keywords: 'automation patterns actions' },
  { id: 'aliases', title: 'Open alias settings', keywords: 'automation command shortcuts' },
  { id: 'macros', title: 'Open macro settings', keywords: 'automation key bindings' },
  {
    id: 'timers',
    title: 'Open timer settings',
    keywords: 'automation recurring commands interval',
  },
  {
    id: 'import',
    title: 'Import from another client…',
    keywords: 'automation tintin mushclient mudlet gmud cmud zmud',
  },
  { id: 'logs', title: 'Open session logs', keywords: 'history search' },
];

export function buildPaletteEntries(deps: PaletteDeps): PaletteEntry[] {
  const entries: PaletteEntry[] = [];

  // The prompt card's rows (P0's palette specimen), found as you type.
  if (deps.openPromptCard) {
    const open = deps.openPromptCard;
    entries.push({
      id: 'prompt-customize',
      section: 'input',
      title: 'Customize prompt…',
      keywords: 'prompt design codes capture template',
      searchOnly: true,
      run: () => open(),
    });
    if (deps.promptDraw !== null && deps.promptDraw !== undefined && deps.setPromptDraw) {
      const draw = deps.promptDraw;
      const setDraw = deps.setPromptDraw;
      entries.push({
        id: 'prompt-draw',
        section: 'input',
        title: 'Draw your prompt',
        keywords: 'prompt design custom own',
        checked: draw,
        searchOnly: true,
        run: () => setDraw(!draw),
      });
    }
    entries.push({
      id: 'prompt-text',
      section: 'input',
      title: 'Edit prompt as text…',
      keywords: 'prompt design template codes',
      searchOnly: true,
      run: () => open('text'),
    });
  }

  // The writing card's rows, one for each kind, named for what you do,
  // so bug finds Report a bug… (Note Editor Q2).
  if (deps.writing) {
    const { kinds, beast, open } = deps.writing;
    const shown: WritingKind[] = [...kinds, 'description', 'history', 'personality', 'purpose'];
    if (beast) shown.push('beast');
    for (const kind of shown) {
      entries.push({
        id: `write-${kind}`,
        section: 'input',
        title: KINDS[kind].palette,
        keywords: `write ${KINDS[kind].keywords}`,
        searchOnly: true,
        run: () => open(kind),
      });
    }
  }

  if (deps.togglePanel) {
    const toggle = deps.togglePanel;
    entries.push({
      id: 'panel',
      section: 'view',
      title: 'Show panel',
      keywords: 'hide sidebar',
      keys: APP_SHORTCUTS.panel,
      checked: deps.panelOpen ?? false,
      run: toggle,
    });
  }
  if (deps.toggleSplit) {
    entries.push({
      id: 'split',
      section: 'view',
      title: 'Split terminal',
      keywords: 'scrollback history scroll back',
      keys: APP_SHORTCUTS.split,
      checked: deps.splitOpen ?? false,
      run: deps.toggleSplit,
    });
  }
  const currentTheme = themeShownBy(THEMES, getCurrentThemeId());
  entries.push({
    id: 'theme',
    section: 'view',
    title: 'Choose theme',
    keywords: 'colors appearance dark light',
    ...(currentTheme ? { meta: currentTheme.label } : {}),
    children: themeEntries,
    childLabel: 'Themes',
    run: () => {},
  });
  for (const pane of deps.paneTypes) {
    entries.push({
      id: `pane-${pane}`,
      section: 'view',
      title: PANE_TITLES[pane],
      keywords: 'pane panel hide',
      checked: deps.paneVisible(pane),
      searchOnly: true,
      run: () => deps.togglePane(pane),
    });
  }
  // Throws away the panes you arranged, so it wears the danger color
  // and stays out of Recent, and the palette never opens on it.
  if (deps.promptShow) {
    const current = deps.promptShow;
    for (const row of PROMPT_SHOW_ROWS) {
      entries.push({
        id: `prompt-show-${row.show}`,
        section: 'view',
        title: row.title,
        keywords: 'prompt pin pinned lift lifted raise band chip bottom where',
        checked: row.show === current,
        searchOnly: true,
        run: () => void sendInput(`#prompt show ${row.show}`),
      });
    }
  }
  entries.push({
    id: 'panel-reset',
    section: 'view',
    title: 'Reset panel layout',
    keywords: 'panes default restore arrangement',
    destructive: true,
    searchOnly: true,
    run: resetPanelLayout,
  });
  entries.push({
    id: 'find',
    section: 'view',
    title: 'Find in scrollback…',
    keywords: 'search',
    keys: APP_SHORTCUTS.find,
    searchOnly: true,
    run: deps.openFind,
  });
  entries.push({
    id: 'help',
    section: 'view',
    title: 'Open help',
    keywords: 'docs manual',
    keys: APP_SHORTCUTS.help,
    searchOnly: true,
    run: deps.openHelp,
  });
  if (deps.openGetStarted) {
    entries.push({
      id: 'get-started',
      section: 'view',
      title: 'Get started',
      keywords: 'walkthrough welcome suggestions presets',
      searchOnly: true,
      run: deps.openGetStarted,
    });
  }
  entries.push({
    id: 'settings',
    section: 'view',
    title: 'Open settings',
    keywords: 'preferences options',
    keys: APP_SHORTCUTS.settings,
    searchOnly: true,
    run: deps.openSettings ?? (() => deps.openSettingsTab('general')),
  });
  for (const tab of SETTINGS_TABS) {
    entries.push({
      id: `settings-${tab.id}`,
      section: 'view',
      title: tab.title,
      keywords: `settings preferences ${tab.keywords}`,
      searchOnly: true,
      run: () => deps.openSettingsTab(tab.id),
    });
  }

  // The rows that move between sessions wait for a second session, as
  // the sidebar does (Q12, Q17).
  const sessions = deps.sessions && deps.sessions.rows.length >= 2 ? deps.sessions : null;
  if (deps.newSession) {
    entries.push({
      id: 'session-new',
      section: 'session',
      title: 'New session…',
      keywords: 'open connection tab',
      keys: APP_SHORTCUTS['session-new'],
      searchOnly: true,
      run: deps.newSession,
    });
  }
  if (sessions) {
    entries.push(
      {
        id: 'session-next',
        section: 'session',
        title: 'Next session',
        keywords: 'step switch tab',
        keys: APP_SHORTCUTS['session-next'],
        searchOnly: true,
        run: () => sessions.step(1),
      },
      {
        id: 'session-previous',
        section: 'session',
        title: 'Previous session',
        keywords: 'step switch tab back',
        keys: APP_SHORTCUTS['session-previous'],
        searchOnly: true,
        run: () => sessions.step(-1),
      },
    );
  }
  if (deps.renameSession) {
    entries.push({
      id: 'session-rename',
      section: 'session',
      title: 'Rename session…',
      keywords: 'name label tab',
      searchOnly: true,
      run: deps.renameSession,
    });
  }
  if (deps.closeSession) {
    entries.push({
      id: 'session-close',
      section: 'session',
      title: 'Close session',
      keywords: 'end remove tab',
      keys: APP_SHORTCUTS['session-close'],
      searchOnly: true,
      run: deps.closeSession,
    });
  }
  if (sessions) {
    entries.push({
      id: 'sessions-sidebar',
      section: 'session',
      title: sessions.shown ? 'Hide sessions' : 'Show sessions',
      keywords: 'sidebar list tabs',
      searchOnly: true,
      run: sessions.toggleShown,
    });
    // Each session to go to, by its label as its row reads, the world
    // beside a session named for its character or its name, and the key
    // that reaches the first nine.
    sessions.rows.forEach((row, i) => {
      const label = sessionLabel(row, sessions.rows);
      entries.push({
        id: `session-goto-${row.id}`,
        section: 'goto',
        title: label.name,
        keywords: 'session go to switch tab',
        ...(label.who && label.place ? { meta: label.place } : {}),
        ...(i < 9 ? { keys: `Mod+${i + 1}` } : {}),
        checked: row.id === sessions.selected,
        searchOnly: true,
        run: () => sessions.goTo(row.id),
      });
    });
  }
  // The snoop rows, while the session has a snoop open (SN8). Next
  // snoop waits for a second tab, Stop for a live one and Close ended
  // snoops for an ended one.
  const snoops = deps.snoops && deps.snoops.tabs.length > 0 ? deps.snoops : null;
  if (snoops) {
    const live = snoops.tabs.filter((tab) => tab.live);
    entries.push({
      id: 'snoop',
      section: 'session',
      title: 'Go to snoop',
      keywords: 'watch player split tab',
      keys: APP_SHORTCUTS.snoop,
      searchOnly: true,
      run: snoops.goTo,
    });
    if (snoops.tabs.length >= 2) {
      entries.push({
        id: 'snoop-next',
        section: 'session',
        title: 'Next snoop',
        keywords: 'watch player step tab',
        searchOnly: true,
        run: snoops.next,
      });
    }
    for (const tab of live) {
      entries.push({
        id: `snoop-stop-${tab.name}`,
        section: 'session',
        title: `Stop snooping ${tab.name}`,
        keywords: 'snoop stop end watch player',
        searchOnly: true,
        run: () => snoops.stop(tab.name),
      });
    }
    if (live.length > 0) {
      entries.push({
        id: 'snoop-stop-all',
        section: 'session',
        title: 'Stop every snoop',
        keywords: 'snoop stop end all watch',
        searchOnly: true,
        run: () => snoops.stop(),
      });
    }
    entries.push({
      id: 'snoop-window',
      section: 'session',
      title: 'Open snoop in a window',
      keywords: 'watch player separate pop out',
      searchOnly: true,
      run: snoops.openWindow,
    });
    if (live.length < snoops.tabs.length) {
      entries.push({
        id: 'snoop-close-ended',
        section: 'session',
        title: 'Close ended snoops',
        keywords: 'snoop close ended tabs',
        searchOnly: true,
        run: snoops.closeEnded,
      });
    }
  }
  entries.push({
    id: 'profile-save',
    section: 'session',
    title: 'Save profile',
    keywords: '#profile',
    searchOnly: true,
    run: () => void sendInput('#profile save'),
  });
  // Disconnect is the final row. The palette opens with its first safe
  // row selected, so ⌘K then Enter can never drop the session. While a
  // redial waits, Connect dials it now and Disconnect ends it.
  if (!deps.connected) {
    entries.push({
      id: 'connect',
      section: 'session',
      title: deps.worldName ? `Connect to ${deps.worldName}` : 'Connect',
      keywords: 'open session login',
      keys: APP_SHORTCUTS.connect,
      run: deps.connect,
    });
  }
  if (deps.connected || deps.redialing) {
    entries.push({
      id: 'disconnect',
      section: 'session',
      title: 'Disconnect',
      keywords: 'quit close session',
      destructive: true,
      run: deps.disconnect,
    });
  }

  return entries;
}

/** Every theme in the gallery's order (Appearance and the menu bar's
 *  Choose theme list them the same way): the gallery's lead, the other
 *  built in themes by name, then your own themes as you added them. */
export function themesInGalleryOrder(): { theme: AppTheme; custom: boolean }[] {
  const custom = THEMES.filter((t) => !BUILTIN_THEMES.includes(t));
  return galleryThemes(BUILTIN_THEMES, custom).map((theme) => ({
    theme,
    custom: custom.includes(theme),
  }));
}

/** Every theme as a submenu row in gallery order, the active one
 *  checked. Picking one follows pickTheme, so while follow system
 *  appearance is on it fills the light or dark slot and shows only when
 *  that matches the OS. The pick applies in every window and saves. */
export function themeEntries(): PaletteEntry[] {
  // A retired id, from a paint cache an older build wrote, shows its
  // successor.
  const current = themeShownBy(THEMES, getCurrentThemeId())?.id;
  return themesInGalleryOrder().map(({ theme: t }) => ({
    id: `theme-${t.id}`,
    section: 'view' as const,
    title: t.label,
    keywords: t.description,
    checked: t.id === current,
    run: () => void chooseTheme(t.id),
  }));
}

export async function chooseTheme(id: string): Promise<void> {
  const prefs = getThemePrefs();
  if (!prefs) {
    // The config has not loaded yet, so there is no pair to fill.
    await applyAndBroadcastTheme(id);
    try {
      await setUiTheme(id);
    } catch (e) {
      console.error('[palette] saving the theme failed', e);
    }
    return;
  }
  const next = pickTheme(prefs, id);
  applyThemePrefs(next, { broadcast: true });
  void broadcastThemePrefs(next);
  try {
    await setUiTheme(next.theme, next);
  } catch (e) {
    console.error('[palette] saving the theme failed', e);
  }
}

/** Fetch the user's aliases as palette rows. Parameterless aliases
 *  run immediately. Ones that read what you type after the name insert
 *  the alias name into the input for the user to finish. A Lua alias
 *  runs its script and ignores its expansion, so its row shows and
 *  searches the script, the way the Aliases list in Settings does. */
export async function buildAliasEntries(deps: PaletteDeps): Promise<PaletteEntry[]> {
  try {
    const json = await exportAliases();
    const parsed: unknown = JSON.parse(json);
    if (!Array.isArray(parsed)) return [];
    const rows: PaletteEntry[] = [];
    for (const raw of parsed) {
      if (!raw || typeof raw !== 'object') continue;
      const r = raw as { name?: unknown; expansion?: unknown; script?: unknown; enabled?: unknown };
      const name = typeof r.name === 'string' ? r.name.trim() : '';
      if (name.length === 0 || r.enabled === false) continue;
      const script = typeof r.script === 'string' ? r.script : null;
      const body = script ?? (typeof r.expansion === 'string' ? r.expansion : '');
      const meta = body.split('\n')[0];
      // A script reads the words after the name from its captures
      // table. Any use of it counts, whatever index the words start at.
      const takesArgs = script !== null ? /\bcaptures\b/.test(script) : /%\d|\$\d/.test(body);
      rows.push({
        id: `alias-${name}`,
        section: 'aliases',
        title: name,
        keywords: body,
        ...(meta ? { meta } : {}),
        metaMono: true,
        searchOnly: true,
        run: () => {
          if (takesArgs) deps.insertInput(`${name} `);
          else void sendInput(name);
        },
      });
    }
    return rows;
  } catch {
    return [];
  }
}

/** Rank entries for a query: title prefix beats title substring beats
 *  keyword substring. Empty query keeps registry order. */
export function filterEntries(entries: PaletteEntry[], query: string): PaletteEntry[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return entries;
  const scored: { score: number; entry: PaletteEntry }[] = [];
  for (const entry of entries) {
    const title = entry.title.toLowerCase();
    const extra = `${entry.keywords ?? ''} ${entry.meta ?? ''}`.toLowerCase();
    let score = -1;
    if (title.startsWith(q)) score = 0;
    else if (title.includes(q)) score = 1;
    else if (extra.includes(q)) score = 2;
    if (score >= 0) scored.push({ score, entry });
  }
  scored.sort((a, b) => a.score - b.score);
  return scored.map((s) => s.entry);
}

// ── Recent ───────────────────────────────────────────────────────────
// The last few commands you ran, newest first, kept in this window's
// storage. Destructive commands never enter it, so a Recent row at the
// top can never be Disconnect.

const RECENT_KEY = 'vosh.palette.recent';
const RECENT_KEEP = 8;
export const RECENT_SHOWN = 3;

export function readRecent(): string[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]');
    return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === 'string') : [];
  } catch {
    return [];
  }
}

export function recordRecent(entry: PaletteEntry): void {
  if (entry.destructive) return;
  try {
    const next = [entry.id, ...readRecent().filter((id) => id !== entry.id)].slice(0, RECENT_KEEP);
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    // storage unavailable. Recent stays empty.
  }
}

export interface PaletteSectionView {
  key: PaletteSection | 'recent';
  label: string;
  rows: PaletteEntry[];
}

/** Lay out the palette. With no query: Recent, then each section's
 *  everyday rows. With a query: every match, ranked, grouped under its
 *  home section in the fixed order, and no Recent. */
export function paletteSections(
  entries: PaletteEntry[],
  query: string,
  recentIds: string[],
): PaletteSectionView[] {
  const out: PaletteSectionView[] = [];
  const searching = query.trim().length > 0;
  const pool = searching ? filterEntries(entries, query) : entries.filter((e) => !e.searchOnly);
  if (!searching) {
    const byId = new Map(entries.map((e) => [e.id, e]));
    const recent = recentIds
      .map((id) => byId.get(id))
      .filter((e): e is PaletteEntry => !!e && !e.destructive && !e.children)
      .slice(0, RECENT_SHOWN);
    if (recent.length > 0) out.push({ key: 'recent', label: SECTION_LABELS.recent, rows: recent });
  }
  for (const section of SECTION_ORDER) {
    const rows = pool.filter((e) => e.section === section);
    if (rows.length > 0) out.push({ key: section, label: SECTION_LABELS[section], rows });
  }
  return out;
}

/** The row the palette selects. With an empty query it is the first
 *  row that is not destructive, or none (-1) when every row is, so
 *  Cmd+K then Enter can never disconnect or reset anything. Once you
 *  type, the best match is selected even when it is destructive,
 *  because you asked for it by name. */
export function initialSelection(rows: PaletteEntry[], query = ''): number {
  if (query.trim().length > 0) return rows.length > 0 ? 0 : -1;
  return rows.findIndex((e) => !e.destructive);
}
