// Where a Settings deep link lands. A target travels as a bare string,
// so localStorage, the goto event, and palette Recent ids need no
// migration. The grammar is `group`, `group:section`, and
// `group:section#anchor`, with `group#anchor` when no section applies.
// For example `automation:macros`, `characters:Ilsabet#tracked` or
// `scripts:vitals_alert`.
// Every tab id the old Settings window used still resolves.

export type SettingsGroup =
  | 'general'
  | 'appearance'
  | 'accessibility'
  | 'layout'
  | 'vitals'
  | 'prompt'
  | 'input'
  | 'automation'
  | 'scripts'
  | 'logs'
  | 'characters';

/** The eleven groups in nav order, with their visible names. `gap`
 *  starts a cluster, which the sidebar sets off with a 13 px gap and no
 *  heading. */
export const SETTINGS_GROUPS: readonly { id: SettingsGroup; label: string; gap?: boolean }[] = [
  { id: 'general', label: 'General' },
  { id: 'appearance', label: 'Appearance' },
  { id: 'accessibility', label: 'Accessibility' },
  { id: 'layout', label: 'Layout', gap: true },
  { id: 'vitals', label: 'Vitals' },
  { id: 'prompt', label: 'Prompt' },
  { id: 'input', label: 'Input' },
  { id: 'automation', label: 'Automation', gap: true },
  { id: 'scripts', label: 'Scripts' },
  { id: 'logs', label: 'Logs', gap: true },
  { id: 'characters', label: 'Characters' },
];

export function settingsGroupLabel(group: SettingsGroup): string {
  return SETTINGS_GROUPS.find((g) => g.id === group)?.label ?? group;
}

export function isSettingsGroup(value: string): value is SettingsGroup {
  return SETTINGS_GROUPS.some((g) => g.id === value);
}

/** A place in Settings. What `section` means depends on the group.
 *  In Automation it is the kind (`triggers`, `timers`, `loadouts`).
 *  In Characters it is a profile name, and no section means the active
 *  profile. In Scripts it is a plugin name, which opens that plugin's
 *  page. Everywhere else it is a section id the page scrolls to.
 *  `anchor` is a row or block inside that. */
export interface SettingsTarget {
  group: SettingsGroup;
  section?: string;
  anchor?: string;
}

// Groups whose section names a kind, a profile or a plugin rather than
// a place to scroll to.
const SECTION_IS_STATE: ReadonlySet<SettingsGroup> = new Set([
  'automation',
  'scripts',
  'characters',
]);

// Groups whose section is a name you gave, a profile or a plugin, so it
// keeps its case. Every other section is an id.
const SECTION_KEEPS_CASE: ReadonlySet<SettingsGroup> = new Set(['scripts', 'characters']);

// Pages that open inside a group, by the section that names them, with
// the title the breadcrumb adds. The group stays active in the nav.
const SETTINGS_SUBPAGES: Readonly<
  Partial<Record<SettingsGroup, Readonly<Record<string, string>>>>
> = {
  logs: { search: 'Search logs', scene: 'Save a scene' },
};

/** The title of the page inside a group that `target` opens, like
 *  `Search logs` for `logs:search`, or null for the group's own
 *  page. A plugin opens its own page under Scripts, titled by its name,
 *  like `vitals_alert` for `scripts:vitals_alert`. */
export function settingsSubpage(target: SettingsTarget): string | null {
  if (!target.section) return null;
  if (target.group === 'scripts') return target.section;
  return SETTINGS_SUBPAGES[target.group]?.[target.section] ?? null;
}

/** Whether a move from `from` to `to` leaves the page you are on: to
 *  another group, or to another page inside the group, like the Scripts
 *  list from a plugin's page. A page with unsaved changes asks before
 *  such a move. */
export function leavesSettingsPage(from: SettingsTarget, to: SettingsTarget): boolean {
  return from.group !== to.group || settingsSubpage(from) !== settingsSubpage(to);
}

// The tab ids the old Settings window used, from the palette, the pane
// menu, and any pending tab left over from an older build.
const LEGACY_TARGETS: Readonly<Record<string, SettingsTarget>> = {
  general: { group: 'general' },
  themes: { group: 'appearance', section: 'theme' },
  typography: { group: 'appearance', section: 'text' },
  vitals: { group: 'vitals' },
  tick: { group: 'automation', section: 'timers', anchor: 'tick' },
  panels: { group: 'characters', anchor: 'layout' },
  profiles: { group: 'characters' },
  loadouts: { group: 'automation', section: 'loadouts' },
  triggers: { group: 'automation', section: 'triggers' },
  aliases: { group: 'automation', section: 'aliases' },
  macros: { group: 'automation', section: 'macros' },
  timers: { group: 'automation', section: 'timers' },
  import: { group: 'automation', anchor: 'import' },
  // The bare link opens the search, as it did in General, so old links
  // land where they always did.
  logs: { group: 'logs', section: 'search' },
};

// Rows that moved out of a section, by the anchor they had there, with
// where they are now. Your prompt left Input, Advanced for its own
// section, and the switch is the section's own row. Values, Meter
// and the warning left Layout, Vitals for Customize vitals. Switch
// themes took the place of Follow system appearance.
const MOVED_ANCHORS: Readonly<Record<string, SettingsTarget>> = {
  'input:advanced#prompt': { group: 'input', section: 'prompt' },
  'input:advanced#prompt-show': { group: 'input', section: 'prompt', anchor: 'prompt-show' },
  'layout:vitals#values': { group: 'layout', section: 'customize-vitals', anchor: 'values' },
  'layout:vitals#meter': { group: 'layout', section: 'customize-vitals', anchor: 'meter' },
  'layout:vitals#warn-low': { group: 'layout', section: 'customize-vitals', anchor: 'warn-low' },
  'appearance:theme#follow-system': {
    group: 'appearance',
    section: 'theme',
    anchor: 'switch-themes',
  },
};

// Sections and rows that left their group when Settings grew to eleven
// groups, keyed by the old link without its anchor, or
// with it for a single row. The anchor rides along unless the key names
// one. Applied after MOVED_ANCHORS, so a link that moved twice lands
// too, like input:advanced#prompt on the Prompt tab. Links live where
// Vosh cannot rewrite them, palette Recent, a pending tab from an older
// build and plugin code, so this table stays for good.
const GROUP_MOVES: Readonly<Record<string, SettingsTarget>> = {
  'general:session-logs': { group: 'logs', section: 'session-logs' },
  'general:scrollback': { group: 'logs', section: 'scrollback' },
  'general:logs': { group: 'logs', section: 'search' },
  'general:scene': { group: 'logs', section: 'scene' },
  'appearance:text#color-vision': {
    group: 'accessibility',
    section: 'color',
    anchor: 'color-vision',
  },
  'appearance:text#fit-game-colors': {
    group: 'accessibility',
    section: 'color',
    anchor: 'fit-game-colors',
  },
  'appearance:text#readable-highlights': {
    group: 'accessibility',
    section: 'color',
    anchor: 'readable-highlights',
  },
  'appearance:advanced#blink-text': {
    group: 'accessibility',
    section: 'motion',
    anchor: 'blink-text',
  },
  'layout:vitals': { group: 'vitals' },
  'layout:customize-vitals': { group: 'vitals', section: 'customize-vitals' },
  'input:prompt': { group: 'prompt' },
  'input:command-line#writing-offer': {
    group: 'input',
    section: 'writing',
    anchor: 'writing-offer',
  },
  'input:command-line#writing-ask-post': {
    group: 'input',
    section: 'writing',
    anchor: 'writing-ask-post',
  },
};

/** Where a target from before Settings had eleven groups lands now. */
function movedSettingsTarget(target: SettingsTarget): SettingsTarget {
  const whole = GROUP_MOVES[formatSettingsTarget(target)];
  if (whole) return { ...whole };
  const bare: SettingsTarget = { group: target.group };
  if (target.section) bare.section = target.section;
  const head = GROUP_MOVES[formatSettingsTarget(bare)];
  if (!head) return target;
  const next: SettingsTarget = { ...head };
  if (target.anchor) next.anchor = target.anchor;
  return next;
}

/** Resolve a deep link string. Legacy tab ids and rows that moved map to
 *  their new place. Anything this cannot read opens General. */
export function resolveSettingsTarget(raw: string): SettingsTarget {
  const text = raw.trim();
  const legacy = LEGACY_TARGETS[text.toLowerCase()];
  if (legacy) return movedSettingsTarget({ ...legacy });

  const hash = text.indexOf('#');
  const head = hash === -1 ? text : text.slice(0, hash);
  const anchor = hash === -1 ? '' : text.slice(hash + 1).trim();
  const colon = head.indexOf(':');
  const group = (colon === -1 ? head : head.slice(0, colon)).trim().toLowerCase();
  const rawSection = colon === -1 ? '' : head.slice(colon + 1).trim();
  if (!isSettingsGroup(group)) return { group: 'general' };

  const target: SettingsTarget = { group };
  const section = SECTION_KEEPS_CASE.has(group) ? rawSection : rawSection.toLowerCase();
  if (section) target.section = section;
  if (anchor) target.anchor = anchor.toLowerCase();
  const moved = MOVED_ANCHORS[formatSettingsTarget(target)];
  return movedSettingsTarget(moved ? { ...moved } : target);
}

/** The string form of a target, the inverse of resolveSettingsTarget. */
export function formatSettingsTarget(target: SettingsTarget): string {
  let out: string = target.group;
  if (target.section) out += `:${target.section}`;
  if (target.anchor) out += `#${target.anchor}`;
  return out;
}

/** The anchors the page should scroll to for `target`, best first:
 *  the anchor, then the section when the section names a place. A
 *  section that opens a page inside the group is not a place. */
export function settingsScrollIds(target: SettingsTarget): string[] {
  const ids: string[] = [];
  if (target.anchor) ids.push(target.anchor);
  if (target.section && !SECTION_IS_STATE.has(target.group) && !settingsSubpage(target)) {
    ids.push(target.section);
  }
  return ids;
}
