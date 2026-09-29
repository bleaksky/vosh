import { exportAliases, sendInput } from './session';
import { PANELS, type PanelId } from './panels';
import { toggleWellSplits, wellSplitsOpen } from './wellSplits';

// Command registry for the ⌘K palette. Commands are built fresh each
// time the palette opens so labels reflect live state (connected,
// splits open, pane visibility). Slash commands run through the same
// backend input pipeline as typed text, so the palette never grows a
// second command implementation.

// ── Shortcut labels ──────────────────────────────────────────────────
// One helper turns a shortcut spec like 'Mod+Shift+L' into the glyphs
// a keycap shows, so the palette, the menus and help never hard-code
// ⌘ where Windows and Linux use Ctrl. Mod is ⌘ on macOS and Ctrl
// elsewhere. The specs are labels only. The window binds the keys.

/** True on macOS. main.tsx tags the root with data-platform before
 *  the first render, and the user agent covers anything earlier. */
export function isMacPlatform(): boolean {
  if (typeof document !== 'undefined') {
    const tag = document.documentElement.dataset.platform;
    if (tag) return tag === 'macos';
  }
  return typeof navigator !== 'undefined' && /Mac/.test(navigator.userAgent);
}

type Modifier = 'ctrl' | 'alt' | 'shift' | 'mod';

// Apple lists modifiers Control, Option, Shift, Command. Windows and
// Linux write Ctrl, Alt, Shift. On those, Mod is Ctrl.
const MAC_MODS: [Modifier, string][] = [
  ['ctrl', '⌃'],
  ['alt', '⌥'],
  ['shift', '⇧'],
  ['mod', '⌘'],
];
const PC_MODS: [Modifier, string][] = [
  ['ctrl', 'Ctrl'],
  ['alt', 'Alt'],
  ['shift', 'Shift'],
];

const NAMED_KEYS: Record<string, [mac: string, pc: string]> = {
  enter: ['↩', 'Enter'],
  return: ['↩', 'Enter'],
  escape: ['Esc', 'Esc'],
  esc: ['Esc', 'Esc'],
  tab: ['⇥', 'Tab'],
  backspace: ['⌫', 'Backspace'],
  delete: ['⌦', 'Delete'],
  space: ['Space', 'Space'],
  up: ['↑', '↑'],
  down: ['↓', '↓'],
  left: ['←', '←'],
  right: ['→', '→'],
  pageup: ['PgUp', 'PgUp'],
  pagedown: ['PgDn', 'PgDn'],
  home: ['Home', 'Home'],
  end: ['End', 'End'],
};

function parseSpec(spec: string): { mods: Set<Modifier>; key: string } {
  // A trailing '++' means the key itself is '+'.
  const plusKey = spec.endsWith('++');
  const parts = (plusKey ? spec.slice(0, -2) : spec).split('+').filter((p) => p.length > 0);
  const key = plusKey ? '+' : (parts.pop() ?? '');
  const mods = new Set<Modifier>();
  for (const raw of parts) {
    const m = raw.toLowerCase();
    if (m === 'mod' || m === 'cmd' || m === 'meta') mods.add('mod');
    else if (m === 'ctrl' || m === 'control') mods.add('ctrl');
    else if (m === 'alt' || m === 'option' || m === 'opt') mods.add('alt');
    else if (m === 'shift') mods.add('shift');
  }
  return { mods, key };
}

/** The keycaps for a shortcut spec, modifiers first in the platform's
 *  order: `shortcutKeys('Mod+Shift+L')` is ⇧ ⌘ L on macOS and
 *  Ctrl Shift L elsewhere. */
export function shortcutKeys(spec: string, mac: boolean = isMacPlatform()): string[] {
  const { mods, key } = parseSpec(spec);
  const out: string[] = [];
  if (mac) {
    for (const [m, glyph] of MAC_MODS) if (mods.has(m)) out.push(glyph);
  } else {
    // Mod is Ctrl here, so Mod+Ctrl collapses to one Ctrl.
    if (mods.has('mod')) mods.add('ctrl');
    for (const [m, name] of PC_MODS) if (mods.has(m)) out.push(name);
  }
  const named = NAMED_KEYS[key.toLowerCase()];
  if (named) out.push(mac ? named[0] : named[1]);
  else if (key.length === 1) out.push(key.toUpperCase());
  else if (key.length > 1) out.push(key[0].toUpperCase() + key.slice(1));
  return out;
}

/** A shortcut as one inline label for menus: ⌘C on macOS, Ctrl+C
 *  elsewhere. */
export function shortcutLabel(spec: string, mac: boolean = isMacPlatform()): string {
  return shortcutKeys(spec, mac).join(mac ? '' : '+');
}

// ── Registry ─────────────────────────────────────────────────────────

export type PaletteGroup = 'commands' | 'panes' | 'settings' | 'aliases';

export interface PaletteEntry {
  id: string;
  group: PaletteGroup;
  title: string;
  /** Dim one-line description shown beside the title. */
  hint?: string;
  /** Display-only keyboard hint (the palette does not bind it). */
  kbd?: string;
  /** Extra match terms beyond the title. */
  keywords?: string;
  run: () => void | Promise<void>;
}

export interface PaletteDeps {
  connected: boolean;
  paneVisible: (id: PanelId) => boolean;
  togglePane: (id: PanelId) => void;
  openHelp: () => void;
  openFind: () => void;
  openSettingsTab: (tab: string) => void;
  connect: () => void;
  disconnect: () => void;
  /** Put text into the input row and focus it (for parameterized
   *  aliases the user finishes typing). */
  insertInput: (text: string) => void;
}

const SETTINGS_TABS: { id: string; hint: string }[] = [
  { id: 'themes', hint: 'theme catalog, editor, terminal palette' },
  { id: 'typography', hint: 'terminal face, size, system fonts' },
  { id: 'vitals', hint: 'hp / mn / mv readout' },
  { id: 'tick', hint: 'tick timer, input chip, moons' },
  { id: 'panels', hint: 'pane placement and tracked affects' },
  { id: 'general', hint: 'input, prompt, updates' },
  { id: 'profiles', hint: 'characters and hosts' },
  { id: 'triggers', hint: 'patterns and actions' },
  { id: 'aliases', hint: 'command shortcuts' },
  { id: 'macros', hint: 'key bindings' },
  { id: 'timers', hint: 'recurring commands on an interval' },
  { id: 'import', hint: 'bring settings from another client' },
  { id: 'logs', hint: 'session history search' },
];

export function buildPaletteEntries(deps: PaletteDeps): PaletteEntry[] {
  const entries: PaletteEntry[] = [];

  entries.push({
    id: 'toggle-splits',
    group: 'commands',
    title: wellSplitsOpen() ? 'close splits' : 'open splits',
    hint: 'session / chat / log panes in the well',
    keywords: 'split tmux pane well',
    run: () => toggleWellSplits(),
  });
  entries.push({
    id: 'find',
    group: 'commands',
    title: 'search scrollback',
    kbd: '⌘F',
    run: deps.openFind,
  });
  entries.push({
    id: 'profile-save',
    group: 'commands',
    title: '#profile save',
    hint: 'persist the live profile',
    run: () => void sendInput('#profile save'),
  });
  entries.push({
    id: 'help',
    group: 'commands',
    title: 'open help',
    keywords: 'docs manual',
    run: deps.openHelp,
  });
  // The session entry goes last in its group. The palette opens with the
  // first row selected, so leading with disconnect meant Cmd+K then Enter
  // dropped the session.
  entries.push(
    deps.connected
      ? {
          id: 'disconnect',
          group: 'commands',
          title: 'disconnect',
          hint: 'close the session',
          run: deps.disconnect,
        }
      : {
          id: 'connect',
          group: 'commands',
          title: 'connect',
          hint: 'open the session',
          run: deps.connect,
        },
  );

  for (const meta of Object.values(PANELS)) {
    entries.push({
      id: `pane-${meta.id}`,
      group: 'panes',
      title: `${deps.paneVisible(meta.id) ? 'hide' : 'show'} ${meta.id} pane`,
      hint: meta.description.toLowerCase(),
      keywords: meta.label,
      run: () => deps.togglePane(meta.id),
    });
  }

  for (const tab of SETTINGS_TABS) {
    entries.push({
      id: `settings-${tab.id}`,
      group: 'settings',
      title: tab.id,
      hint: tab.hint,
      keywords: 'settings preferences options',
      run: () => deps.openSettingsTab(tab.id),
    });
  }

  return entries;
}

/** Fetch the user's aliases as palette rows. Parameterless aliases
 *  run immediately; ones whose template references captures insert
 *  the alias name into the input for the user to finish. */
export async function buildAliasEntries(deps: PaletteDeps): Promise<PaletteEntry[]> {
  try {
    const json = await exportAliases();
    const parsed: unknown = JSON.parse(json);
    if (!Array.isArray(parsed)) return [];
    const rows: PaletteEntry[] = [];
    for (const raw of parsed) {
      if (!raw || typeof raw !== 'object') continue;
      const r = raw as { name?: unknown; template?: unknown; enabled?: unknown };
      const name = typeof r.name === 'string' ? r.name.trim() : '';
      if (name.length === 0 || r.enabled === false) continue;
      const template = typeof r.template === 'string' ? r.template : '';
      const takesArgs = /%\d|\$\d/.test(template);
      rows.push({
        id: `alias-${name}`,
        group: 'aliases',
        title: name,
        hint: template,
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
 *  hint/keyword substring. Empty query keeps registry order. */
export function filterEntries(entries: PaletteEntry[], query: string): PaletteEntry[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return entries;
  const scored: { score: number; entry: PaletteEntry }[] = [];
  for (const entry of entries) {
    const title = entry.title.toLowerCase();
    const extra = `${entry.hint ?? ''} ${entry.keywords ?? ''}`.toLowerCase();
    let score = -1;
    if (title.startsWith(q)) score = 0;
    else if (title.includes(q)) score = 1;
    else if (extra.includes(q)) score = 2;
    if (score >= 0) scored.push({ score, entry });
  }
  scored.sort((a, b) => a.score - b.score);
  return scored.map((s) => s.entry);
}
