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
  | 'layout'
  | 'input'
  | 'automation'
  | 'scripts'
  | 'characters';

/** The seven groups in nav order, with their visible names. */
export const SETTINGS_GROUPS: readonly { id: SettingsGroup; label: string }[] = [
  { id: 'general', label: 'General' },
  { id: 'appearance', label: 'Appearance' },
  { id: 'layout', label: 'Layout' },
  { id: 'input', label: 'Input' },
  { id: 'automation', label: 'Automation' },
  { id: 'scripts', label: 'Scripts' },
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
  general: { logs: 'Session logs' },
};

/** The title of the page inside a group that `target` opens, like
 *  `Session logs` for `general:logs`, or null for the group's own
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
  vitals: { group: 'layout', section: 'vitals' },
  tick: { group: 'automation', section: 'timers', anchor: 'tick' },
  panels: { group: 'characters', anchor: 'layout' },
  profiles: { group: 'characters' },
  loadouts: { group: 'automation', section: 'loadouts' },
  triggers: { group: 'automation', section: 'triggers' },
  aliases: { group: 'automation', section: 'aliases' },
  macros: { group: 'automation', section: 'macros' },
  timers: { group: 'automation', section: 'timers' },
  import: { group: 'automation', anchor: 'import' },
  logs: { group: 'general', section: 'logs' },
};

// Rows that moved out of a section, by the anchor they had there, with
// where they are now. Your prompt left Input, Advanced for its own
// section (P12), and the switch is the section's own row. Values, Meter
// and the warning left Layout, Vitals for Customize vitals (Vitals
// Styles Q3).
const MOVED_ANCHORS: Readonly<Record<string, SettingsTarget>> = {
  'input:advanced#prompt': { group: 'input', section: 'prompt' },
  'input:advanced#prompt-show': { group: 'input', section: 'prompt', anchor: 'prompt-show' },
  'layout:vitals#values': { group: 'layout', section: 'customize-vitals', anchor: 'values' },
  'layout:vitals#meter': { group: 'layout', section: 'customize-vitals', anchor: 'meter' },
  'layout:vitals#warn-low': { group: 'layout', section: 'customize-vitals', anchor: 'warn-low' },
};

/** Resolve a deep link string. Legacy tab ids and rows that moved map to
 *  their new place. Anything this cannot read opens General. */
export function resolveSettingsTarget(raw: string): SettingsTarget {
  const text = raw.trim();
  const legacy = LEGACY_TARGETS[text.toLowerCase()];
  if (legacy) return { ...legacy };

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
  return moved ? { ...moved } : target;
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
