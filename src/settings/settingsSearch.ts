import { PRESET_CATEGORIES, PRESETS } from '../automation/presets';
import {
  formatSettingsTarget,
  settingsGroupLabel,
  type SettingsGroup,
  type SettingsTarget,
} from '../lib/settingsNav';

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
  // General, from the approved board.
  {
    label: 'World',
    keywords: 'connect connection mud server session port build',
    target: at('general', 'connection', 'world'),
  },
  {
    label: 'Host and port',
    keywords: 'connect connection address server session',
    target: at('general', 'connection', 'host'),
  },
  {
    label: 'Use TLS',
    keywords: 'connect connection secure ssl encrypted session',
    target: at('general', 'connection', 'tls'),
  },
  {
    label: 'Reconnect when the link drops',
    description:
      'Vosh dials up to 8 times over about 5 minutes, and you log in yourself. The game closes its login prompt after about 2 minutes.',
    keywords: 'reconnect redial auto automatic drop dropped link dead disconnect retry',
    target: at('general', 'connection', 'reconnect'),
  },
  {
    label: 'Check for updates',
    keywords: 'update version install restart',
    target: at('general', 'updates'),
  },
  {
    label: 'Check for updates when Vosh opens',
    keywords: 'update automatic launch start',
    target: at('general', 'updates', 'auto-update'),
  },
  {
    label: 'Keep the same for every character',
    description: 'Turn one off and each character keeps its own.',
    keywords: 'scope global profile share theme font size keep last command updates',
    target: at('general', 'scope'),
  },
  {
    label: 'Session logs',
    keywords: 'log logs saved sessions history lines',
    target: at('general', 'session-logs'),
  },
  {
    label: 'Log sessions',
    keywords: 'log logging record save sessions localhost',
    target: at('general', 'session-logs', 'log-sessions'),
  },
  {
    label: 'Keep logs for',
    keywords: 'log logs retention delete old days year forever space disk',
    target: at('general', 'session-logs', 'keep-logs'),
  },
  {
    label: 'Scrollback size',
    keywords: 'scrollback history lines terminal buffer memory',
    target: at('general', 'scrollback', 'scrollback-size'),
  },
  {
    label: 'Search logs',
    keywords: 'log history find copy text save file export download txt colors',
    target: at('general', 'logs'),
  },
  {
    label: 'GPU rendering',
    keywords: 'advanced webgl performance renderer graphics',
    target: at('general', 'advanced', 'gpu'),
    only: 'not-macos',
  },

  // Appearance, from the approved board.
  {
    label: 'Theme',
    keywords: 'colors palette gallery dark light nord ember rubric vellum one everforest',
    target: at('appearance', 'theme'),
  },
  {
    label: 'Import a theme',
    description: 'Vosh reads Ghostty, iTerm2, Kitty, and Alacritty themes.',
    keywords: 'file',
    target: at('appearance', 'theme', 'import-theme'),
  },
  {
    label: 'Switch themes',
    description: "Turns at the game's dawn and dusk, about every 6 minutes.",
    keywords:
      'follow system appearance dark mode light mode automatic with the system with the game day night',
    target: at('appearance', 'theme', 'switch-themes'),
  },
  { label: 'Light theme', target: at('appearance', 'theme', 'light-theme') },
  { label: 'Dark theme', target: at('appearance', 'theme', 'dark-theme') },
  { label: 'Day theme', target: at('appearance', 'theme', 'day-theme') },
  { label: 'Night theme', target: at('appearance', 'theme', 'night-theme') },
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
    label: 'Fit game colors',
    description:
      'While you play, Vosh lifts the game colors that fade on the theme, and Settings keeps the theme as published.',
    keywords: 'contrast faint legible readable ansi room names published play',
    target: at('appearance', 'text', 'fit-game-colors'),
  },
  {
    label: 'Color vision',
    description:
      'Vosh swaps the colors your eyes confuse for colors they tell apart, the way color blind modes in games do.',
    keywords:
      'color blind colorblind deuteranopia protanopia tritanopia red green blue yellow orange violet purple pink swap mode cvd accessibility danger warn success status window hue',
    target: at('appearance', 'text', 'color-vision'),
  },
  {
    label: 'Keep highlight colors readable',
    description:
      'Vosh darkens or lightens a color your triggers set when the theme would make it faint.',
    keywords: 'contrast trigger highlight faint legible true color hex',
    target: at('appearance', 'text', 'readable-highlights'),
  },
  {
    label: 'Collapse repeated lines',
    description: 'A line the same as the line before it shows once, with a count in front.',
    keywords: 'spam duplicate repeat repeated same lines count squash fold compress',
    target: at('appearance', 'text', 'collapse-repeats'),
  },
  {
    label: 'In a fight',
    description: 'Every line that arrives while you are fighting.',
    keywords: 'collapse repeated combat battle round target count',
    target: at('appearance', 'text', 'collapse-fights'),
  },
  {
    label: 'Attack lines',
    description: 'Each hit and miss the game shows you, in a fight or not.',
    keywords: 'collapse repeated damage hits misses verbs dismembers combat count',
    target: at('appearance', 'text', 'collapse-attacks'),
  },
  // Panel text shows Font and Size, named here for the panel so search
  // tells them from the terminal's.
  {
    label: 'Panel font',
    description: 'Every pane and the status line under the terminal draw in it.',
    keywords:
      'typeface family panel text panes status line map affects group chat vitals designed terminal system',
    target: at('appearance', 'panel-text', 'panel-font'),
  },
  {
    label: 'Panel size',
    description: 'The headers, the rows, and the status line grow with it.',
    keywords:
      'font size points panel text panes status line map affects group chat vitals bigger smaller',
    target: at('appearance', 'panel-text', 'panel-size'),
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
    label: 'Blinking text',
    description:
      'Text your MUD or prompt sets to blink flashes. It starts off if your system reduces motion.',
    keywords: 'advanced blink flash sgr reduce motion animation',
    target: at('appearance', 'advanced', 'blink-text'),
  },
  {
    label: 'Font stack',
    description: 'Vosh uses the first font in this list that you have.',
    keywords: 'advanced family fallback css typeface',
    target: at('appearance', 'advanced', 'font-stack'),
  },

  // Layout, from the approved board.
  {
    label: 'Show the panel',
    description: 'When you hide it, your vitals move to the status line.',
    keywords: 'hide panel sidebar right',
    target: at('layout', 'panel', 'show-panel'),
  },
  {
    label: 'Width',
    description: "You can also drag the panel's edge.",
    keywords: 'panel width size wide narrow points',
    target: at('layout', 'panel', 'panel-width'),
  },
  {
    label: 'Panes and tracked affects',
    description: 'Vosh saves these for each character.',
    keywords: 'panel panes layout map affects characters',
    target: at('layout', 'panel', 'panes'),
  },
  {
    label: 'Style',
    description: 'Each tile draws your vitals in one style. Pick the one your panel shows.',
    keywords:
      'vitals style gallery rows one line ledger gauges pips bands ladders blocks traces dials rings vials orbs candles text look density compact health mana moves',
    target: at('layout', 'vitals', 'style'),
  },
  {
    label: 'Show your vitals in',
    description: 'Status line moves them under the terminal, and the panes take the room.',
    keywords: 'vitals place panel status line footer where health mana moves',
    target: at('layout', 'vitals', 'place'),
  },
  {
    label: 'Hide vitals while your prompt is pinned',
    description:
      'While your prompt is pinned, the panes take their room. Turn it off if your prompt leaves your vitals out.',
    keywords: 'vitals hide pinned prompt band panel footer health mana moves',
    target: at('layout', 'vitals', 'hide-pinned'),
  },
  {
    label: 'Vitals and their order',
    description: 'Drag a vital to move it, give it a color, or turn it off.',
    keywords:
      'customize vitals order drag move reorder colors colours swatch hide show off reset health mana moves',
    target: at('layout', 'customize-vitals', 'vitals-order'),
  },
  {
    label: 'Your opponent',
    description: 'In a fight, its name and its health in warn, in every style.',
    keywords: 'customize vitals opponent enemy mob fight top bottom health',
    target: at('layout', 'customize-vitals', 'opponent'),
  },
  {
    label: 'Values',
    description: 'Current drops the maximum. Percent matches the Group pane.',
    keywords: 'vitals numbers current max maximum percent percentage health mana moves',
    target: at('layout', 'customize-vitals', 'values'),
  },
  {
    label: 'Meter',
    description: 'Bar is easier to read in a fight. None keeps only the numbers.',
    // Not `line`, which would pull Meter into a search for One line
    // through the `one` in None.
    keywords: 'vitals bar gauge thick thin health mana moves',
    target: at('layout', 'customize-vitals', 'meter'),
  },
  {
    label: 'Warn before you run low',
    description:
      "Vitals turn yellow under two thirds and red under one third, like your group's health.",
    keywords: 'vitals low warning danger thirds yellow red color health mana moves',
    target: at('layout', 'customize-vitals', 'warn-low'),
  },
  {
    label: 'Show each hit',
    description:
      'A hit leaves the part it took pale for a moment, then it drains away. Works in every style with a fill.',
    keywords: 'vitals hit trail pale drain heal peak health mana moves',
    target: at('layout', 'customize-vitals', 'show-each-hit'),
  },
  {
    label: 'Divider color',
    keywords: 'split terminal scrollback divider line',
    target: at('layout', 'split', 'divider-color'),
  },
  {
    label: 'Tick and time',
    description: 'How the tick, the game time, and the moons show in the status line.',
    keywords: 'chip style caption icon value clock sun moon moons phase',
    target: at('layout', 'status', 'tick-time'),
  },
  {
    label: 'Game time',
    description: 'How the game time shows in the status line, like 18:00 or 6:00 PM.',
    keywords: 'clock 12 24 hour hours am pm time of day military',
    target: at('layout', 'status', 'game-time'),
  },
  {
    label: 'Tick counts',
    description:
      'Up shows the seconds since the last tick and Down the seconds left until the next. Down waits at 0 when the game is late, and Down past 0 keeps counting below zero until the tick lands.',
    keywords: 'tick count countdown count down up direction reverse late negative minus below zero',
    target: at('layout', 'status', 'tick-counts'),
  },
  // The Affects card sits above Vitals on the page. Its rows come last
  // here, since search breaks a tie by this order and `chip style`
  // should still find Tick and time, whose saved name it is.
  {
    label: 'Style',
    description:
      'Timers first keeps your slots, Countdown sorts by hours left, Grouped chips puts what to recast first, and Draining chips colors only the hours a chip has left.',
    keywords: 'affects pane layout timers first countdown grouped draining chips drain list order',
    target: at('layout', 'affects', 'affects-style'),
  },
  {
    label: 'Marker',
    description:
      'It sits beside each affect you track, and its color shows whether the affect is up, running out, or missing.',
    keywords: 'affects dot circle square plus minus none mark indicator',
    target: at('layout', 'affects', 'affects-marker'),
  },
  {
    label: 'Tint what to recast',
    description:
      'A missing affect sits on a red wash, and one about to drop sits on yellow or red.',
    keywords: 'affects tint wash background recast missing expiring drop color',
    target: at('layout', 'affects', 'affects-tint'),
  },
  {
    label: 'Running out at',
    description:
      "With this many hours or fewer an affect's hours turn yellow, and one you track counts as running out.",
    keywords: 'affects warn warning threshold hours running out yellow expiring soon',
    target: at('layout', 'affects', 'affects-running-out'),
  },
  {
    label: 'Almost gone at',
    description:
      "With this many hours or fewer the hours turn bold red. The game's own affects bar turns red at 1.",
    keywords: 'affects almost gone red threshold hours danger critical last',
    target: at('layout', 'affects', 'affects-almost-gone'),
  },

  // Input, from the approved board.
  {
    label: 'Caret shape',
    keywords: 'cursor block outline underline pipe command line',
    target: at('input', 'command-line', 'caret'),
  },
  {
    label: 'Keep last command',
    description: 'Your last command stays in the line, selected, so Enter sends it again.',
    keywords: 'history resend repeat',
    target: at('input', 'command-line', 'keep-last'),
  },
  {
    label: 'Check spelling when you chat',
    description: 'Vosh checks only lines that start with say, tell, reply, or a channel name.',
    keywords: 'spell check spelling',
    target: at('input', 'command-line', 'spellcheck'),
  },
  {
    label: 'Offer the card when the game’s editor opens',
    description:
      'Type note edit or description edit and Vosh offers to open it in its writing card.',
    keywords: 'writing card editor note description history notice offer',
    target: at('input', 'command-line', 'writing-offer'),
  },
  {
    label: 'Ask before you post',
    description:
      'Turn this off and Post posts your note at once, unless a report would record a room other than the one you began it in.',
    keywords: 'writing card note post confirm ask sure',
    target: at('input', 'command-line', 'writing-ask-post'),
  },
  {
    label: 'Mark your commands',
    description:
      'Draws a grey › before each command you send, except after a prompt that already ends in >.',
    keywords: 'echo caret arrow prefix typed input sent',
    target: at('input', 'command-line', 'mark-commands'),
  },
  {
    label: 'Sent command color',
    keywords: 'echo local command typed input',
    target: at('input', 'command-line', 'sent-color'),
  },
  {
    label: 'Show the commands your macros send',
    keywords: 'macro echo keys',
    target: at('input', 'command-line', 'echo-macros'),
  },
  {
    label: 'Wait between pasted lines',
    keywords: 'advanced paste pacing delay flood milliseconds',
    target: at('input', 'advanced', 'paste-delay'),
  },

  // Input, Prompt (P12).
  {
    label: "Your game's prompt",
    description: 'Your prompt setting in the game. Vosh reads its codes.',
    keywords: 'prompt codes setting fight prompt fprompt capture pattern point line',
    target: at('input', 'prompt', 'prompt-game'),
  },
  {
    label: 'Draw your own prompt',
    description: 'It takes the place of the prompt the game sends.',
    keywords: 'custom prompt design template customize gag replace preview',
    target: at('input', 'prompt'),
  },
  {
    label: 'Where your prompt shows',
    description: 'Your prompt shows in the text, where the game sends it.',
    keywords: 'prompt pin pinned lift lifted raise band chip bottom',
    target: at('input', 'prompt', 'prompt-show'),
  },

  // Automation, from the approved board.
  {
    label: 'Triggers',
    keywords:
      'trigger pattern highlight gag replace route wash regex text starts with match mode alert banner notification sound tone chime bell knock bounce flash',
    target: at('automation', 'triggers'),
  },
  {
    label: 'Aliases',
    keywords: 'alias shortcut expansion command',
    target: at('automation', 'aliases'),
  },
  {
    label: 'Macros',
    keywords: 'macro key binding keyboard shortcut numpad',
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
    keywords:
      'preset triggers macros built in numpad movement walk keys alerts tells name attacked health connection banner notification sound',
    target: at('automation', 'presets'),
  },
  // Each preset of the library, on its own card, which holds its colors,
  // Your changes and Reset to preset (Presets review). The alert presets
  // come from Rust, so the Presets row finds them.
  ...PRESETS.map(
    (preset): SettingsRowEntry => ({
      label: preset.name,
      description: preset.description,
      keywords: [
        'preset',
        PRESET_CATEGORIES[preset.category].toLowerCase(),
        Object.keys(preset.colors).length > 0 ? 'colors swatch swatches' : '',
        'your changes edits edited reset to preset',
      ]
        .filter(Boolean)
        .join(' '),
      target: at('automation', 'presets', `presets:${preset.id}`),
    }),
  ),
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

  // Scripts, from the approved boards. A section here names a plugin,
  // so each row is an anchor on the list page.
  {
    label: 'Plugins',
    keywords: 'lua script plugin install new',
    target: at('scripts', undefined, 'plugins'),
  },
  {
    label: 'Console',
    keywords: 'lua print run output errors',
    target: at('scripts', undefined, 'console'),
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
    label: 'Import a profile',
    keywords: 'import export toml share',
    target: at('characters', undefined, 'import-profile'),
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
    description: 'The Affects pane marks these and shows any you are missing.',
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
