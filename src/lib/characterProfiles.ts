import type { LoginClaim, SessionIdentity } from '../ipc/characters';
import type { ProfileAutoMatch, ProfileEntry } from '../ipc/profiles';
import type { SessionRow } from '../ipc/session';
import { KNOWN_WORLDS, knownWorld, worldLabel, worldName } from './knownWorlds';
import { sessionLabel } from './sessionLabel';
import { countWord, listJoin, possessive } from './text';

// The words and choices Settings > Characters builds from the profile
// index and the sessions: display names, the login toggle's character,
// the World select, the profiles the sessions play, and the sentences
// the page reports. Pure, so the page stays about layout.

/** The reserved profile every install starts with. */
export const DEFAULT_PROFILE = 'default';

/** `Default` for the reserved profile, else the name as typed. */
export function profileDisplayName(name: string): string {
  return name === DEFAULT_PROFILE ? 'Default' : name;
}

/** The profile a deep link names: the exact name, else one that
 *  matches it in any case, else one whose display name matches (so
 *  `Default` finds `default`). Null when none does. */
export function findProfileName(names: readonly string[], wanted: string): string | null {
  const want = wanted.trim();
  if (want.length === 0) return null;
  if (names.includes(want)) return want;
  const lower = want.toLowerCase();
  return (
    names.find((n) => n.toLowerCase() === lower) ??
    names.find((n) => profileDisplayName(n).toLowerCase() === lower) ??
    null
  );
}

/** The character the login toggle names: the profile's first
 *  character, else the one logged in now, else null (the toggle waits
 *  until a character is known). */
export function loginCharacter(
  autoMatch: ProfileAutoMatch | null | undefined,
  identity: SessionIdentity | null,
): string | null {
  const first = autoMatch?.characters?.map((c) => c.trim()).find((c) => c.length > 0);
  if (first) return first;
  const live = identity?.character?.trim();
  return live ? live : null;
}

export function loginLabel(character: string | null): string {
  return character
    ? `Use this profile when you log in as ${character}`
    : 'Use this profile when you log in';
}

/** Whether the profile names a world to connect to. */
export function hasWorld(autoMatch: ProfileAutoMatch | null | undefined): boolean {
  return (autoMatch?.host?.trim().length ?? 0) > 0;
}

// ---------------------------------------------------------------
// World select
// ---------------------------------------------------------------

/** The World select's value for a profile with no world. */
export const NO_WORLD = '';

export interface WorldOption {
  value: string;
  label: string;
  host: string | null;
  port: number | null;
}

/** The select value for a host and port. A known world on its own port
 *  (or with no port) is one choice however its host is spelled. */
export function worldKey(host: string | null | undefined, port: number | null | undefined): string {
  const clean = host ? host.trim().toLowerCase().replace(/\.$/, '') : '';
  if (clean.length === 0) return NO_WORLD;
  const known = knownWorld(clean);
  if (known && (port === null || port === undefined || port === known.port)) {
    return `world:${known.domain}`;
  }
  return `host:${clean}:${port ?? ''}`;
}

/** A host and port where the World select finds its choices: every
 *  profile's world, the connection you saved, and the one you are on. */
export interface WorldSource {
  host: string | null | undefined;
  port: number | null | undefined;
}

/** The World select's choices: every known world, then every other
 *  host a source names in the order first seen, then No world. A known
 *  world on a port not its own reads as its row does, `The Forsaken
 *  Lands 1825`, and any other host with its port. */
export function worldOptions(sources: readonly WorldSource[]): WorldOption[] {
  const out: WorldOption[] = KNOWN_WORLDS.map((w) => ({
    value: `world:${w.domain}`,
    label: w.name,
    host: w.host,
    port: w.port,
  }));
  const seen = new Set(out.map((o) => o.value));
  for (const source of sources) {
    const value = worldKey(source.host, source.port);
    if (value === NO_WORLD || seen.has(value)) continue;
    seen.add(value);
    const host = (source.host ?? '').trim().replace(/\.$/, '');
    const port = source.port ?? null;
    let label = port === null ? host : `${host}:${port}`;
    if (knownWorld(host) && port !== null) label = worldLabel(host, port);
    out.push({ value, label, host, port });
  }
  out.push({ value: NO_WORLD, label: 'No world', host: null, port: null });
  return out;
}

/** The sources for worldOptions from the profile index, the saved
 *  connection, and the session. */
export function worldSources(
  profiles: readonly ProfileEntry[],
  saved: WorldSource | null,
  identity: SessionIdentity | null,
): WorldSource[] {
  const out: WorldSource[] = profiles.map((p) => ({
    host: p.auto_match?.host,
    port: p.auto_match?.port,
  }));
  if (saved) out.push(saved);
  if (identity) out.push({ host: identity.host, port: identity.port });
  return out;
}

/** The characters an export of `entry` can name: those it has on its
 *  world. A profile with no world has none, since a character logs in
 *  only on a world. */
export function exportCharacters(entry: ProfileEntry | undefined): string[] {
  if (!hasWorld(entry?.auto_match)) return [];
  return (entry?.auto_match?.characters ?? []).map((c) => c.trim()).filter((c) => c !== '');
}

/** The world meta a profile row shows, in two parts so a tight row
 *  ends the world in an ellipsis and keeps the port. */
export interface ProfileWorld {
  /** Like `The Forsaken Lands`. */
  world: string;
  /** A known world's port when it is not the world's own, like 1825 on
   *  the build port, else null. */
  port: number | null;
}

/** The world meta a profile row shows, `The Forsaken Lands` on the
 *  world's own port and `The Forsaken Lands 1825` on another. Any other
 *  host shows as typed. Null when the profile has no world. */
export function profileWorld(entry: ProfileEntry): ProfileWorld | null {
  const host = entry.auto_match?.host?.trim();
  if (!host) return null;
  const port = entry.auto_match?.port ?? null;
  const own = knownWorld(host)?.port;
  return { world: worldName(host), port: own !== undefined && port !== own ? port : null };
}

// ---------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------

/** The profiles the open sessions play. */
export interface PlayedProfiles {
  /** Every profile a session plays, which the list marks with a dot and
   *  Delete refuses. */
  all: ReadonlySet<string>;
  /** The profile the selected session plays, which Switch to this
   *  profile is dimmed for, since that session already plays it. */
  selected: string;
}

/** The profiles `rows` play, and the one session `selected` plays.
 *  Before the first session list both are `active`, the profile in use. */
export function playedProfiles(
  rows: readonly SessionRow[],
  selected: number,
  active: string,
): PlayedProfiles {
  const all = new Set(rows.flatMap((row) => (row.profile ? [row.profile] : [])));
  if (all.size === 0) all.add(active);
  return { all, selected: rows.find((row) => row.id === selected)?.profile ?? active };
}

/** The port a session's row shows beside its name, ` on 1825`, while
 *  another open session goes by the same name, so the line tells the
 *  two apart as the sidebar does. Empty otherwise. */
function portApart(row: SessionRow, rows: readonly SessionRow[]): string {
  const { who, meta } = sessionLabel(row, rows);
  const same = rows.filter((r) => sessionLabel(r, rows).who === who);
  return meta && same.length > 1 ? ` on ${meta}` : '';
}

/** A session as the line under the list names it on its own: `Tolliver's
 *  session`, `a session on The Forsaken Lands 1825` at the login, or `a
 *  new session` before it has a place. After the first profile a name
 *  drops the word session, `Orla's`, as the line says it once. */
function sessionPhrase(row: SessionRow, rows: readonly SessionRow[], first: boolean): string {
  const { who, place } = sessionLabel(row, rows);
  if (who) return `${possessive(who)}${first ? ' session' : ''}${portApart(row, rows)}`;
  return place ? `a session on ${place}` : 'a new session';
}

/** A session among others on one profile: `Builder`, or `one on The
 *  Forsaken Lands 1825` at the login. */
function sessionInList(row: SessionRow, rows: readonly SessionRow[]): string {
  const { who, place } = sessionLabel(row, rows);
  if (who) return `${who}${portApart(row, rows)}`;
  return place ? `one on ${place}` : 'a new one';
}

/** The line under the Characters list while two or more sessions are
 *  open, naming the sessions on each profile they play in list order.
 *  One session on each reads `Default plays in Tolliver's session, Build
 *  in Orla's.`, and a profile two or more play reads `Default plays in
 *  two sessions, Tolliver and Builder.`, a sentence for each profile.
 *  Null with one session, where the dot says it all. */
export function sessionsSentence(
  profiles: readonly ProfileEntry[],
  rows: readonly SessionRow[],
): string | null {
  if (rows.length < 2) return null;
  const groups = profiles
    .map((p) => ({
      name: profileDisplayName(p.name),
      on: rows.filter((r) => r.profile === p.name),
    }))
    .filter((group) => group.on.length > 0);
  if (groups.length === 0) return null;
  if (groups.every((group) => group.on.length === 1)) {
    const clauses = groups.map(({ name, on }, i) =>
      i === 0
        ? `${name} plays in ${sessionPhrase(on[0], rows, true)}`
        : `${name} in ${sessionPhrase(on[0], rows, false)}`,
    );
    return `${clauses.join(', ')}.`;
  }
  return groups
    .map(({ name, on }) => {
      if (on.length === 1) return `${name} plays in ${sessionPhrase(on[0], rows, true)}.`;
      const count = countWord(on.length).toLowerCase();
      return `${name} plays in ${count} sessions, ${listJoin(on.map((r) => sessionInList(r, rows)))}.`;
    })
    .join(' ');
}

// ---------------------------------------------------------------
// Names and sentences
// ---------------------------------------------------------------

/** What turning a login toggle on took from other profiles, like
 *  `Vosh moved Ilsabet from Test-Prompt to Ilsabet.` Null when it took
 *  nothing. */
export function movedSentence(
  character: string,
  releasedFrom: readonly string[],
  to: string,
): string | null {
  if (releasedFrom.length === 0) return null;
  const from = listJoin(releasedFrom.map(profileDisplayName));
  return `Vosh moved ${character} from ${from} to ${profileDisplayName(to)}.`;
}

/** What a pin did: where the character now plays each profile, then each
 *  pinned claim's new port and every other character that moved with it,
 *  like `Tolliver plays Default on 1848 and Build on 1825. Default's
 *  claim now sits on 1848.` One toggle pins every claim to the same
 *  port, the world's own. */
function pinnedSentence(character: string, claim: LoginClaim, to: string): string | null {
  const port = claim.entry.auto_match?.port;
  if (claim.pinned.length === 0 || port == null) return null;
  const pinnedTo = claim.pinned[0].port;
  const pinnedNames = listJoin(claim.pinned.map((pin) => profileDisplayName(pin.profile)));
  const sentences = [
    `${character} plays ${pinnedNames} on ${pinnedTo} and ${profileDisplayName(to)} on ${port}.`,
  ];
  const self = character.trim().toLowerCase();
  for (const pin of claim.pinned) {
    const owner = possessive(profileDisplayName(pin.profile));
    const others = pin.characters.filter((c) => c.trim().toLowerCase() !== self);
    sentences.push(
      others.length > 0
        ? `${owner} claim now sits on ${pin.port}, and ${listJoin(others)} moved with it.`
        : `${owner} claim now sits on ${pin.port}.`,
    );
  }
  return sentences.join(' ');
}

/** What turning a login toggle on did to other profiles, the claims it
 *  pinned and then the profiles it took the character from, for the
 *  line under the list. Null when it did neither. */
export function loginSentence(character: string, claim: LoginClaim, to: string): string | null {
  const said = [
    pinnedSentence(character, claim, to),
    movedSentence(character, claim.released_from, to),
  ].filter((s): s is string => s !== null);
  return said.length > 0 ? said.join(' ') : null;
}

function taken(names: readonly string[]): Set<string> {
  return new Set(names.map((n) => n.toLowerCase()));
}

/** The name a new profile starts with: the character you are logged in
 *  as when no profile has that name yet, else nothing. */
export function newProfileName(identity: SessionIdentity | null, names: readonly string[]): string {
  const character = identity?.character?.trim();
  if (!character || taken(names).has(character.toLowerCase())) return '';
  return character;
}

/** The profile that already has `typed` as its name in any case, else
 *  null. Profile files live on disks that ignore case, so `Default`
 *  belongs to `default` and `HEALER` to `Healer`. A rename passes the
 *  profile's own name as `renaming`, which may change its own case. */
export function takenProfileName(
  names: readonly string[],
  typed: string,
  renaming?: string,
): string | null {
  const want = typed.trim().toLowerCase();
  if (want.length === 0) return null;
  return names.find((n) => n !== renaming && n.toLowerCase() === want) ?? null;
}

/** The sentence for a name another profile has, the same one the
 *  backend sends. */
export function takenSentence(name: string): string {
  return `You already have a profile named ${profileDisplayName(name)}.`;
}

/** Whether a typed rename leaves the profile as it reads now: its own
 *  name, or `Default` for the reserved profile. */
export function keepsProfileName(name: string, typed: string): boolean {
  const clean = typed.trim();
  return clean === name || clean === profileDisplayName(name);
}

/** A free name for a copy of `source`: `Ilsabet copy`, then `Ilsabet
 *  copy 2` and on. */
export function copyName(source: string, names: readonly string[]): string {
  const used = taken(names);
  const base = `${profileDisplayName(source)} copy`;
  if (!used.has(base.toLowerCase())) return base;
  for (let n = 2; ; n += 1) {
    const candidate = `${base} ${n}`;
    if (!used.has(candidate.toLowerCase())) return candidate;
  }
}

/** Character names typed as a list: split on commas, trimmed, blanks
 *  and repeats in another case dropped. */
export function parseCharacterNames(text: string): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const part of text.split(',')) {
    const name = part.trim();
    const key = name.toLowerCase();
    if (name.length === 0 || seen.has(key)) continue;
    seen.add(key);
    out.push(name);
  }
  return out;
}

export function formatCharacterNames(names: readonly string[] | undefined): string {
  return (names ?? []).join(', ');
}

/** A typed port: null for blank, the number for a TCP port, undefined
 *  for anything else. */
export function parsePort(text: string): number | null | undefined {
  const clean = text.trim();
  if (clean.length === 0) return null;
  if (!/^\d+$/.test(clean)) return undefined;
  const port = Number(clean);
  return port >= 1 && port <= 65535 ? port : undefined;
}

/** The login claim a new profile starts with: the world you are on (or
 *  the active profile's) and the character you are logged in as. The
 *  toggle stays off here. The page turns it on with profileSetLogin so
 *  the new profile takes the character from any other. A world with no
 *  character stays off so the profile never matches every login. */
export function newProfileClaim(
  identity: SessionIdentity | null,
  active: ProfileEntry | undefined,
): ProfileAutoMatch | null {
  const host = identity?.host ?? active?.auto_match?.host ?? null;
  if (!host || host.trim().length === 0) return null;
  const port = identity ? identity.port : (active?.auto_match?.port ?? null);
  const character = identity?.character?.trim();
  return { host, port, characters: character ? [character] : [], enabled: false };
}
