// The picker inside the prompt card: every field Vosh can draw, grouped
// by topic, with what each reads now on the right, search over names,
// other words and the game's codes, and the line that says where a
// field comes from. Pure, so PromptPicker.tsx stays about layout.

import type { PromptFieldGroup, PromptFieldState } from '../ipc/prompt';

/** A row of the picker: a field, or a layout item of Text and layout. */
export type PickerRow =
  | { kind: 'field'; field: PromptFieldState; status: string; dim: boolean }
  | { kind: 'layout'; id: LayoutId; label: string; status: string; dim: false };

export type LayoutId = 'nl' | 'nl_fight' | 'space' | 'right';

export interface PickerGroup {
  id: PromptFieldGroup | 'layout';
  label: string;
  rows: PickerRow[];
}

const GROUP_LABELS: Record<PromptFieldGroup, string> = {
  vitals: 'Vitals',
  fight: 'Fight',
  group: 'Group',
  character: 'Character',
  worth: 'Worth',
  affects: 'Affects',
  room: 'Room',
  time_and_sky: 'Time and sky',
  vosh: 'Vosh',
  scripts: 'Your scripts',
  building: 'Building',
  more: 'More from the game',
};

/** The topics in the order the picker lists them. Text and layout comes
 *  before More from the game, which ends the list on every host. */
const ORDER: (PromptFieldGroup | 'layout')[] = [
  'vitals',
  'fight',
  'group',
  'character',
  'worth',
  'affects',
  'room',
  'time_and_sky',
  'vosh',
  'scripts',
  'building',
  'layout',
  'more',
];

const LAYOUT: { id: LayoutId; label: string }[] = [
  { id: 'nl', label: 'Line break' },
  { id: 'nl_fight', label: 'Line break in a fight' },
  { id: 'space', label: 'Space' },
  { id: 'right', label: 'Push to the right edge' },
];

/** What Edit as text takes for each layout item, at its caret. */
export const LAYOUT_TOKENS: Record<LayoutId, string> = {
  nl: '%nl',
  nl_fight: '%{if:fight}%nl%{end}',
  space: ' ',
  right: '%{right}',
};

/** The field reads nothing until your prompt in the game shows its code:
 *  it comes only from the prompt, or from a package only the new server
 *  build sends that has not come this session. */
export function needsCode(f: PromptFieldState): boolean {
  if (f.codes.length === 0 || f.in_prompt) return false;
  return f.package === null || (f.new_build && !f.sent);
}

/** What a row reads on the right, where a menu puts shortcuts: the value
 *  now, an enum as its word, or why it has none. */
export function rowStatus(f: PromptFieldState): string {
  if (f.state === 'value') return f.value ?? '';
  if (f.state === 'hidden') return 'hidden';
  if (f.group === 'fight' && f.state === 'absent') return 'in a fight';
  if (needsCode(f)) return 'not in your prompt';
  if (f.package !== null && !f.sent) return 'not sent yet';
  return '';
}

/** The line under a field's name in the formats pane. */
export function sourceLine(f: PromptFieldState): string {
  if (needsCode(f)) return `Add ${f.codes[0]} to your prompt in the game to use it.`;
  const fromPrompt = f.codes.length > 0;
  const fromGame = f.package !== null;
  if (fromPrompt && fromGame) {
    return 'From your prompt, and from the game when your prompt leaves it out.';
  }
  if (fromPrompt) return 'From your prompt only. The game sends it nowhere else.';
  // The game's prompt is the game's own line, which Vosh keeps.
  if (f.kind === 'raw') return 'From the game. Vosh keeps your last prompt as it came.';
  if (f.group === 'scripts') return 'From your scripts.';
  if (f.group === 'vosh') return 'From Vosh.';
  return 'From the game.';
}

/** The words a search matches for a field: its label, its name and other
 *  spellings, its search words, and its codes. */
function matches(f: PromptFieldState, query: string): boolean {
  const q = query.trim();
  if (q.length === 0) return true;
  // A game code matches as typed, since %x is experience and %X to next
  // level.
  if (q.startsWith('%')) return f.codes.some((c) => c.startsWith(q));
  const lower = q.toLowerCase();
  return [f.label, f.name, ...f.aliases, ...f.search, ...f.codes].some((w) =>
    w.toLowerCase().includes(lower),
  );
}

/** Whether the Building topic shows: for immortals, whom the game shows
 *  building values or the staff queues. */
function building(catalog: readonly PromptFieldState[], packages: readonly string[]): boolean {
  return (
    packages.includes('Imm.Queues') ||
    catalog.some((f) => f.group === 'building' && (f.state === 'value' || f.in_prompt))
  );
}

/** The picker's topics and rows for `query`. A topic with no row that
 *  matches is left out, Your scripts shows only once a script set a
 *  value, and Building only for immortals. */
export function pickerGroups(
  catalog: readonly PromptFieldState[],
  packages: readonly string[],
  query: string,
): PickerGroup[] {
  const groups: PickerGroup[] = [];
  const showBuilding = building(catalog, packages);
  for (const id of ORDER) {
    if (id === 'layout') {
      const q = query.trim().toLowerCase();
      const rows = LAYOUT.filter((l) => q === '' || l.label.toLowerCase().includes(q)).map(
        (l): PickerRow => ({ kind: 'layout', id: l.id, label: l.label, status: '', dim: false }),
      );
      if (rows.length > 0) groups.push({ id, label: 'Text and layout', rows });
      continue;
    }
    if (id === 'building' && !showBuilding) continue;
    const rows = catalog
      // In a fight is what When writes, not a value to show.
      .filter((f) => f.group === id && f.listed && f.name !== 'fight' && matches(f, query))
      .map(
        (f): PickerRow => ({
          kind: 'field',
          field: f,
          status: rowStatus(f),
          dim: needsCode(f),
        }),
      );
    if (rows.length > 0) groups.push({ id, label: GROUP_LABELS[id], rows });
  }
  return groups;
}

/** Every row in order, for the keyboard. */
export function flatRows(groups: readonly PickerGroup[]): PickerRow[] {
  return groups.flatMap((g) => g.rows);
}

/** A row's key. */
export function rowKey(row: PickerRow): string {
  return row.kind === 'field' ? `field:${row.field.name}` : `layout:${row.id}`;
}

/** The field a template names for a row: its name, with the parameter
 *  you typed for a field that takes one (`aff:sanctuary`). Null while a
 *  parameter is still missing. */
export function fieldName(f: PromptFieldState, param: string): string | null {
  if (!f.param) return f.name;
  const p = param.trim().replace(/\s+/g, '_');
  return p.length > 0 ? `${f.name}:${p}` : null;
}

/** What a field that takes a name asks for, as the field's label and
 *  placeholder. */
export function paramPrompt(f: PromptFieldState): { label: string; placeholder: string } {
  if (f.name === 'aff') return { label: 'Affect', placeholder: 'sanctuary' };
  if (f.name === 'queue') return { label: 'Queue', placeholder: 'bugs' };
  if (f.name === 'gmcp') return { label: 'Path', placeholder: 'Char.Vitals.hp' };
  return { label: 'Member', placeholder: 'Quenby' };
}
