import {
  formatSettingsTarget,
  settingsGroupLabel,
  type SettingsGroup,
  type SettingsTarget,
} from './settingsNav';

// The Settings search index. Search finds rows, not groups: each entry
// is one row or block with its label, the words people remember it
// by, and the target it opens. A page marks the matching element with
// the same anchor (Row `anchor`, Section `id`), and the frame scrolls
// to it and flashes it. When a page does not draw the anchor, the
// frame falls back to the section. Add an entry here with every row a
// page adds.

export interface SettingsRowEntry {
  /** The label the page shows, in sentence case. */
  label: string;
  /** The row description, when the page shows one. */
  description?: string;
  /** More words that should find the row. */
  keywords?: string;
  target: SettingsTarget;
  /** Rows that exist only in loadout mode, or only off macOS. */
  only?: 'path-b' | 'not-macos';
}

export interface SettingsSearchEnv {
  /** Loadout mode (Path B) is on. */
  pathB: boolean;
  mac: boolean;
}

const at = (group: SettingsGroup, section?: string, anchor?: string): SettingsTarget => {
  const target: SettingsTarget = { group };
  if (section) target.section = section;
  if (anchor) target.anchor = anchor;
  return target;
};

export const SETTINGS_ROWS: readonly SettingsRowEntry[] = [
  // General has no approved board yet. These point at its interim
  // sections.
  {
    label: 'Check for updates',
    keywords: 'update version install restart launch',
    target: at('general', 'updates'),
  },
  {
    label: 'Share settings across profiles',
    keywords: 'scope global profile theme font keep last command',
    target: at('general', 'scope'),
  },
  {
    label: 'Session logs',
    keywords: 'log search history copy',
    target: at('general', 'logs'),
  },
  {
    label: 'GPU rendering',
    keywords: 'webgl performance renderer',
    target: at('general', 'rendering'),
    only: 'not-macos',
  },

  // Appearance, from the approved board.
  {
    label: 'Theme',
    keywords: 'colors palette gallery dark light nord ember vellum',
    target: at('appearance', 'theme'),
  },
  {
    label: 'Import a theme',
    description: 'Vosh reads Ghostty, iTerm2, Kitty, and Alacritty themes.',
    keywords: 'file',
    target: at('appearance', 'theme', 'import-theme'),
  },
  {
    label: 'Follow system appearance',
    description: 'Vosh switches between your light and dark theme when macOS does.',
    keywords: 'dark mode light mode automatic',
    target: at('appearance', 'theme', 'follow-system'),
  },
  { label: 'Light theme', target: at('appearance', 'theme', 'light-theme') },
  { label: 'Dark theme', target: at('appearance', 'theme', 'dark-theme') },
  {
    label: 'Font',
    keywords: 'typeface family terminal text monospace',
    target: at('appearance', 'text', 'font'),
  },
  {
    label: 'Size',
    keywords: 'font size points terminal text bigger smaller',
    target: at('appearance', 'text', 'size'),
  },
  {
    label: 'Line height',
    keywords: 'spacing compact loose terminal text',
    target: at('appearance', 'text', 'line-height'),
  },
  {
    label: "Use the theme's colors for MUD text",
    description: 'Turn this off to keep the exact colors your MUD sends.',
    keywords: 'ansi tint',
    target: at('appearance', 'text', 'theme-colors'),
  },
  {
    label: 'Custom themes',
    description: 'Start from the theme you see now, then change any color.',
    keywords: 'advanced new edit delete rename theme editor',
    target: at('appearance', 'advanced', 'custom-theme'),
  },
  {
    label: 'Base palette',
    description: "MUD text uses these 16 colors when you turn off the theme's colors.",
    keywords: 'advanced terminal ansi',
    target: at('appearance', 'advanced', 'base-palette'),
  },
  {
    label: 'Bright text in bold',
    description: 'On macOS, bright colors draw in the bold weight of your font.',
    keywords: 'advanced ansi',
    target: at('appearance', 'advanced', 'bright-bold'),
  },
  {
    label: 'Font stack',
    description: 'Vosh uses the first font in this list that you have.',
    keywords: 'advanced family fallback css typeface',
    target: at('appearance', 'advanced', 'font-stack'),
  },
  {
    label: 'Split divider color',
    description: 'The line between live output and scrollback while you split the terminal.',
    keywords: 'advanced',
    target: at('appearance', 'advanced', 'divider-color'),
  },
  {
    label: 'Sent command color',
    description: 'The color of each command you send when the terminal shows it.',
    keywords: 'advanced echo local',
    target: at('appearance', 'advanced', 'sent-color'),
  },

  // Layout has no approved board yet.
  {
    label: 'Panel and panes',
    keywords: 'panel pane split vitals',
    target: at('layout', 'panel'),
  },

  // Input has no approved board yet. These point at its interim rows.
  {
    label: 'Keep last command',
    keywords: 'history enter resend',
    target: at('input', 'command-line', 'keep-last'),
  },
  {
    label: 'Spell check chat lines',
    keywords: 'spelling',
    target: at('input', 'command-line', 'spellcheck'),
  },
  {
    label: 'Caret shape',
    keywords: 'cursor block underline pipe',
    target: at('input', 'command-line', 'caret'),
  },
  {
    label: 'Echo macro commands',
    keywords: 'macros echo',
    target: at('input', 'command-line', 'echo-macros'),
  },
  {
    label: 'Paste pacing',
    keywords: 'paste delay flood lines milliseconds',
    target: at('input', 'command-line', 'paste-pacing'),
  },
  {
    label: 'Custom prompt',
    keywords: 'prompt template gag replace',
    target: at('input', 'prompt', 'prompt-template'),
  },

  // Automation, from the approved board.
  {
    label: 'Triggers',
    keywords: 'trigger pattern highlight gag replace route wash regex',
    target: at('automation', 'triggers'),
  },
  {
    label: 'Aliases',
    keywords: 'alias shortcut expansion command',
    target: at('automation', 'aliases'),
  },
  {
    label: 'Macros',
    keywords: 'macro key binding keyboard shortcut',
    target: at('automation', 'macros'),
  },
  {
    label: 'Timers',
    keywords: 'timer interval repeat every seconds',
    target: at('automation', 'timers'),
  },
  {
    label: 'Tick',
    keywords: 'tick timer warn auto fire reset',
    target: at('automation', 'timers', 'tick'),
  },
  {
    label: 'Presets',
    keywords: 'preset triggers built in',
    target: at('automation', 'presets'),
  },
  {
    label: 'Loadouts',
    keywords: 'loadout groups active catalog',
    target: at('automation', 'loadouts'),
    only: 'path-b',
  },
  {
    label: 'Import from another client',
    keywords: 'import tintin mushclient mudlet gmud cmud zmud migrate',
    target: at('automation', undefined, 'import'),
  },
  {
    label: 'Edit all as JSON',
    keywords: 'json raw bulk',
    target: at('automation', undefined, 'json'),
  },

  // Characters, from the approved board. No section means the active
  // profile.
  {
    label: 'Profiles',
    keywords: 'profile character rename duplicate delete switch',
    target: at('characters'),
  },
  {
    label: 'New profile',
    keywords: 'profile character create add',
    target: at('characters', undefined, 'new-profile'),
  },
  {
    label: 'Use this profile when you log in',
    keywords: 'auto match login character automatic switch',
    target: at('characters', undefined, 'login'),
  },
  {
    label: 'World',
    keywords: 'mud host port server',
    target: at('characters', undefined, 'world'),
  },
  {
    label: 'Tracked affects',
    description: 'The Affects pane lists these first and marks any you are missing.',
    keywords: 'affects spells missing',
    target: at('characters', undefined, 'tracked'),
  },
  {
    label: 'Panel layout',
    description: 'Vosh saves the panes you arrange for each character.',
    keywords: 'panes reset default',
    target: at('characters', undefined, 'layout'),
  },
];

/** A stable key for a row, its target as a deep link string. */
export function settingsRowKey(row: SettingsRowEntry): string {
  return formatSettingsTarget(row.target);
}

const fold = (text: string) =>
  text.toLowerCase().normalize('NFKD').replace(/[̀-ͯ]/g, '').replace(/\s+/g, ' ').trim();

/** Rows matching `query`, best first. Every word must appear in the
 *  label, description, keywords, or group name. A label that starts
 *  with the query ranks first, then a label word that starts with it,
 *  then a label that contains it, then everything else, each in index
 *  order. An empty query finds nothing. */
export function searchSettingsRows(
  query: string,
  env: SettingsSearchEnv,
  rows: readonly SettingsRowEntry[] = SETTINGS_ROWS,
): SettingsRowEntry[] {
  const q = fold(query);
  if (!q) return [];
  const words = q.split(' ');
  const hits: { row: SettingsRowEntry; score: number; index: number }[] = [];
  rows.forEach((row, index) => {
    if (row.only === 'path-b' && !env.pathB) return;
    if (row.only === 'not-macos' && env.mac) return;
    const label = fold(row.label);
    const haystack = fold(
      [row.label, row.description, row.keywords, settingsGroupLabel(row.target.group)]
        .filter(Boolean)
        .join(' '),
    );
    if (!words.every((word) => haystack.includes(word))) return;
    const score = label.startsWith(q)
      ? 0
      : label.split(' ').some((word) => word.startsWith(q))
        ? 1
        : label.includes(q)
          ? 2
          : 3;
    hits.push({ row, score, index });
  });
  hits.sort((a, b) => a.score - b.score || a.index - b.index);
  return hits.map((hit) => hit.row);
}
