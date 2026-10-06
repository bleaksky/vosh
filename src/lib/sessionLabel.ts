import type { SessionRow } from '../ipc/session';
import { hostKey, knownWorld, worldLabel, worldName } from './knownWorlds';

// How Vosh names a session wherever it shows one, the sessions sidebar,
// the title band, the window title and Settings, by Q7 of the Sessions
// review. A session goes by the name you gave it, else its character,
// else where it plays, else New session. Its port shows when it tells
// the session apart: on a known world a port that is not the world's
// own, and on any other host any port while another open session
// shares that host. label_of in src-tauri/src/sessions.rs names it the
// same way for a banner and the line another session prints, and both
// run fixtures/session-labels/cases.json.

/** What a session's label reads from its row. */
export type LabelSource = Pick<SessionRow, 'id' | 'name' | 'character' | 'host' | 'port'>;

export interface SessionLabel {
  /** What the session goes by. */
  name: string;
  /** The name you gave it, else its character, null with neither. */
  who: string | null;
  /** Where it plays, the world with its port when the port shows, like
   *  `The Forsaken Lands 1825`. Null before it has an address. */
  place: string | null;
  /** The port, in quiet meta beside a name that is not the place. */
  meta: string | null;
  /** A name that is the place with its port, in its two parts, so a
   *  tight row ends the world in an ellipsis and keeps the port. */
  split: { world: string; port: string } | null;
}

/** Whether another open session plays on `session`'s host. */
function sharesHost(session: LabelSource, rows: readonly LabelSource[]): boolean {
  const key = session.host === null ? null : hostKey(session.host);
  return rows.some(
    (row) => row.id !== session.id && row.host !== null && hostKey(row.host) === key,
  );
}

/** Where `session` plays, the world and the port when the port shows. */
function placeOf(
  session: LabelSource,
  rows: readonly LabelSource[],
): { world: string; port: string | null } | null {
  const { host, port } = session;
  if (host === null || port === null) return null;
  const world = worldName(host);
  const label =
    knownWorld(host) || !sharesHost(session, rows) ? worldLabel(host, port) : `${world} ${port}`;
  return { world, port: label === world ? null : String(port) };
}

/** The name a rename field gives a session that read `reads` as the
 *  field opened on it. Undefined while the text still reads the same,
 *  so a Return on it changes nothing, null for a blank field, which
 *  clears the name, and else the text without its outer spaces. */
export function typedName(text: string, reads: string): string | null | undefined {
  const name = text.trim();
  if (name === reads) return undefined;
  return name || null;
}

/** The label of `session` among the open sessions in `rows`. */
export function sessionLabel(session: LabelSource, rows: readonly LabelSource[]): SessionLabel {
  const who = session.name?.trim() || session.character?.trim() || null;
  const at = placeOf(session, rows);
  const place = at && (at.port ? `${at.world} ${at.port}` : at.world);
  const port = at?.port ?? null;
  return {
    name: who ?? place ?? 'New session',
    who,
    place,
    meta: who && port,
    split: !who && at && port ? { world: at.world, port } : null,
  };
}
