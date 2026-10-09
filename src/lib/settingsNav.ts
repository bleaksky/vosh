// Where a Settings deep link lands. A target travels as a bare string,
// through localStorage, the goto event and palette Recent ids. The
// grammar is `group`, `group:section`, and `group:section#anchor`, with
// `group#anchor` when no section applies. For example
// `automation:macros`, `characters:Ilsabet#tracked` or
// `scripts:vitals_alert`.

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

/** Resolve a deep link string. Anything this cannot read opens
 *  General. */
export function resolveSettingsTarget(raw: string): SettingsTarget {
  const text = raw.trim();
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
  return target;
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
