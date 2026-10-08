import type { ProfileEntry } from '../ipc/profiles';
import type { SessionRow } from '../ipc/session';
import { profileDisplayName } from './characterProfiles';
import { hostKey, knownWorld, worldLabel } from './knownWorlds';
import { sessionLabel } from './sessionLabel';
import { listJoin, possessive } from './text';

// What the New session form says under its Profile row. Which profile
// the form picks comes from Rust, through profileBeforeLogin, where
// ProfileSet::resolve_before_login sits beside the claim matcher a
// login runs.

/** Whether `profile`'s login claim is pinned to `host` and `port`, as
 *  the hint under the Profile row says. A claim whose login is off never
 *  counts. Hosts compare as Rust compares them. */
function pinnedTo(profile: ProfileEntry, host: string, port: number): boolean {
  const claim = profile.auto_match;
  if (!claim?.host || claim.enabled === false) return false;
  return hostKey(claim.host) === hostKey(host) && claim.port === port;
}

/** What the form says under its Profile row. `hint` names a profile
 *  another session plays, else a pin to this host and port. `warn` names
 *  another session already connected to the world's own port, where you
 *  play, when the form dials that port too. A note, never a refusal. The
 *  build port keeps its own player files, so it never shows the note. */
export interface ProfileLines {
  hint: string | null;
  warn: string | null;
}

/** The first example of HELP MULTI, help 57 in the game's own data. */
const MULTI = 'HELP MULTI lists “Having more than one character logged on at once.”';

/** Every session but `session` that plays `profile`. */
function playing(rows: readonly SessionRow[], session: number, profile: string): SessionRow[] {
  return rows.filter((row) => row.id !== session && row.profile === profile);
}

/** Whether the form shows its Profile row: with more than one profile,
 *  or when another session plays the one it picks. */
export function showsProfileRow(
  profiles: readonly ProfileEntry[],
  rows: readonly SessionRow[],
  session: number,
  profile: string,
): boolean {
  return profiles.length > 1 || playing(rows, session, profile).length > 0;
}

/** The names `others` go by in a sentence, or null when one of them has
 *  no name or character yet. */
function names(others: readonly SessionRow[], rows: readonly SessionRow[]): string[] | null {
  const who = others.map((row) => sessionLabel(row, rows).who);
  return who.every((name): name is string => name !== null) ? who : null;
}

/** The line about other sessions that play `name`. */
function sharedLine(
  others: readonly SessionRow[],
  rows: readonly SessionRow[],
  name: string,
): string {
  const who = names(others, rows);
  if (others.length === 1) {
    const owner = who ? `${possessive(who[0])} session` : 'Another session';
    return `${owner} plays ${name} too. An edit in either reaches both.`;
  }
  const owners = who
    ? `${listJoin(who.map(possessive))} sessions`
    : `${others.length} other sessions`;
  return `${owners} play ${name} too. An edit in any of them reaches all.`;
}

/** The note about other sessions connected to this world. */
function multiLine(others: readonly SessionRow[], rows: readonly SessionRow[]): string {
  const who = names(others, rows);
  let subject: string;
  if (others.length === 1) subject = `${who ? who[0] : 'Another session'} is`;
  else subject = `${who ? listJoin(who) : `${others.length} other sessions`} are`;
  return `${subject} connected to this world. ${MULTI}`;
}

/** What the form for `session`, on `host` and `port` with `profile`
 *  picked, says under its Profile row. */
export function profileLines(
  profiles: readonly ProfileEntry[],
  rows: readonly SessionRow[],
  session: number,
  profile: string,
  host: string,
  port: number,
): ProfileLines {
  const name = profileDisplayName(profile);
  const others = playing(rows, session, profile);
  const entry = profiles.find((p) => p.name === profile);
  let hint: string | null = null;
  if (others.length > 0) hint = sharedLine(others, rows, name);
  else if (entry && pinnedTo(entry, host, port)) {
    const place = knownWorld(host) ? worldLabel(host, port) : `${host.trim()}:${port}`;
    hint = `${name} is pinned to ${place}.`;
  }
  const world = knownWorld(host);
  const connected =
    world && port === world.port
      ? rows.filter(
          (row) =>
            row.id !== session &&
            row.connected &&
            row.host !== null &&
            row.port === port &&
            knownWorld(row.host)?.domain === world.domain,
        )
      : [];
  return { hint, warn: connected.length > 0 ? multiLine(connected, rows) : null };
}
