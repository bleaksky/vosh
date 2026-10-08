import type { ImportAddAs, ImportLuaItem, ImportPreview, ImportResult } from '../../ipc/characters';
import type { ProfileEntry } from '../../ipc/profiles';
import { profileDisplayName } from '../../lib/characterProfiles';
import { listJoin, possessive } from '../../lib/text';
import { PANE_LABELS } from '../../panel/paneTypes';

// What the import sheet under Characters says about a Vosh profile
// export (board 5 of the Scripts design, Scripts Q9 and Q10): what the
// file holds, the note under a character another profile has, and the
// line under the list once the import is done. Pure, so the sheet stays
// about layout.

/** A file you picked to import, as Vosh read it. */
export interface ImportFile {
  /** The name you picked it under, like `Healer profile.toml`. */
  fileName: string;
  text: string;
  preview: ImportPreview;
}

/** The warning over a file whose triggers or aliases run Lua, the one
 *  Install gives a plugin. */
export const LUA_WARNING =
  'Lua in a trigger or an alias can send commands to the game and read everything the game sends. Import profiles only from people you trust.';

/** A piece of a summary value: plain text, or a name in the MUD font. */
export type SummaryPart = string | { mono: string };

/** One label and value pair of In this file. */
export interface SummaryRow {
  label: string;
  value: SummaryPart[];
  /** Runs across both columns. */
  wide: boolean;
}

// The pane menu's names, and an unknown pane type as Rust sent it.
const PANE_NAMES: Readonly<Record<string, string | undefined>> = PANE_LABELS;

function timers({ timers: count, tick }: ImportPreview): string {
  if (!tick) return String(count);
  return count === 0 ? 'The tick' : `${count} and the tick`;
}

/** `Trigger tells, alias heal`, each name in the MUD font. */
function luaItems(items: readonly ImportLuaItem[]): SummaryPart[] {
  return items.flatMap((item, i) => [
    i === 0 ? `${item.kind[0].toUpperCase()}${item.kind.slice(1)} ` : `, ${item.kind} `,
    { mono: item.name },
  ]);
}

/** `vitals_alert, off until you turn it on`. */
function plugins(names: readonly string[]): SummaryPart[] {
  const parts = names.flatMap((name, i): SummaryPart[] =>
    i === 0 ? [{ mono: name }] : [', ', { mono: name }],
  );
  return [...parts, `, off until you turn ${names.length === 1 ? 'it' : 'them'} on`];
}

/** In this file, in the order board 5 draws it. Runs Lua and Plugins
 *  show only for a file that has some. In loadout mode a file that holds
 *  presets closes with a line that says the catalog keeps its own
 *  (Presets board 5). */
export function importSummary(preview: ImportPreview): SummaryRow[] {
  const count = (label: string, n: number): SummaryRow => ({
    label,
    value: [String(n)],
    wide: false,
  });
  const panes = preview.panes.map((pane) => PANE_NAMES[pane] ?? pane);
  const rows: SummaryRow[] = [
    count('Triggers', preview.triggers),
    count('Aliases', preview.aliases),
    count('Macros', preview.macros),
    { label: 'Timers', value: [timers(preview)], wide: false },
    count('Variables', preview.variables),
    { label: 'Panes', value: [panes.length === 0 ? 'None' : panes.join(', ')], wide: false },
  ];
  if (preview.runs_lua.length > 0) {
    rows.push({ label: 'Runs Lua', value: luaItems(preview.runs_lua), wide: true });
  }
  if (preview.plugins.length > 0) {
    rows.push({ label: 'Plugins', value: plugins(preview.plugins), wide: true });
  }
  if (preview.presets_stay) {
    rows.push({ label: 'Presets', value: ['Stay as the catalog has them'], wide: true });
  }
  return rows;
}

/** The warn note under a character `claimant` has. Moving the last
 *  character a profile has turns its login off, so the note says so
 *  when your list shows `claimant` with its login on and no other
 *  character. */
export function claimNote(
  character: string,
  claimant: string,
  profiles: readonly ProfileEntry[],
): string {
  const owner = profileDisplayName(claimant);
  const move = `${owner} uses ${character} now. Turn this on to move ${character} here`;
  const claim = profiles.find((p) => p.name === claimant)?.auto_match;
  const others = (claim?.characters ?? []).filter((c) => {
    const name = c.trim().toLowerCase();
    return name !== '' && name !== character.toLowerCase();
  });
  const lastOne = others.length === 0 && claim?.enabled !== false;
  return lastOne ? `${move}, and ${possessive(owner)} login turns off.` : `${move}.`;
}

/** Group `items` by the profile each names, in the order first seen. */
function byProfile<T extends { character: string; profile: string }>(
  items: readonly T[],
): [string, T[]][] {
  const groups = new Map<string, T[]>();
  for (const item of items) groups.set(item.profile, [...(groups.get(item.profile) ?? []), item]);
  return [...groups];
}

/** The kinds of item a clash names, in the order Rust lists them. */
const CLASH_KINDS = ['trigger', 'alias', 'macro'] as const;
const CLASH_PLURALS = { trigger: 'triggers', alias: 'aliases', macro: 'macros' } as const;

/** How many clashes the line names before it counts them instead, so a
 *  file you import twice never reads as a list of everything in it. */
const NAMED_CLASHES = 3;

/** The sentence that says the catalog kept your own item wherever the
 *  file had one of the same name, or a macro on the same key (Q26). */
function clashSentence(clashes: ImportResult['clashes']): string {
  const kinds = CLASH_KINDS.map((kind) => ({
    kind,
    names: clashes.filter((c) => c.kind === kind).map((c) => c.name),
  })).filter(({ names }) => names.length > 0);
  const parts =
    clashes.length <= NAMED_CLASHES
      ? kinds.map(
          ({ kind, names }) =>
            `the ${names.length === 1 ? kind : CLASH_PLURALS[kind]} ${listJoin(names)}`,
        )
      : kinds.map(({ kind, names }) => `${names.length} of its ${CLASH_PLURALS[kind]}`);
  return `You already had ${listJoin(parts)}, so Vosh kept yours.`;
}

/** The line under the list once an import of `fileName` is done.
 *  `logins` are the characters you left on, so a new profile that took
 *  none starts with its login off. */
export function importedSentence(
  fileName: string,
  addAs: ImportAddAs,
  result: ImportResult,
  logins: readonly string[],
): string {
  const name = profileDisplayName(result.name);
  const parts = [
    addAs === 'replace'
      ? `Vosh replaced ${name} with ${fileName}.`
      : `Vosh added ${name} from ${fileName}.`,
  ];
  if (result.catalog_group !== null) {
    parts.push(
      `Its triggers, aliases and macros joined the catalog in the group ${result.catalog_group}.`,
    );
  }
  if (result.clashes.length > 0) parts.push(clashSentence(result.clashes));
  if (addAs === 'replace') {
    parts.push(`${name} keeps its world and characters.`);
    return parts.join(' ');
  }
  for (const [profile, moved] of byProfile(result.moved_from)) {
    const who = listJoin(moved.map((m) => m.character));
    const off = moved.some((m) => m.login_off)
      ? `, and ${possessive(profileDisplayName(profile))} login is off`
      : '';
    parts.push(`${who} now ${moved.length === 1 ? 'uses' : 'use'} ${name}${off}.`);
  }
  const kept = byProfile(result.kept_with);
  kept.forEach(([profile, stay], i) => {
    const who = listJoin(stay.map((k) => k.character));
    const last = i === kept.length - 1 && logins.length === 0;
    const off = last ? `, so ${name} starts with its login off` : '';
    const verb = stay.length === 1 ? 'stays' : 'stay';
    parts.push(`${who} ${verb} with ${profileDisplayName(profile)}${off}.`);
  });
  return parts.join(' ');
}
