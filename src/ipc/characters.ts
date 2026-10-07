// One profile as Settings shows it under Characters, its login claim and
// world, its export, and who is logged in.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { sanitizeLayout, type PaneLayout } from '../panel/paneLayout';
import { normalizeTrackedAffects, type TrackedAffect } from './affects';
import { PROFILE_CHANGED, SESSION_IDENTITY_CHANGED } from './events';
import type { ProfileAutoMatch, ProfileEntry } from './profiles';

// Characters. Settings > Characters edits any profile in place, active
// or not, and never switches the live session. A command that takes an
// optional profile acts on the live profile when you leave it out or
// name the active one, and on the inactive profile's file otherwise.
// Every edit to a profile's detail announces itself as
// vosh://profile-changed with the profile's name. An inactive edit never
// fires vosh://tracked-affects-changed or vosh://pane-layout-changed,
// which carry the live profile's data to the main window.

/** One profile as the Characters group shows it. */
export interface ProfileDetail {
  name: string;
  /** `Default` for the reserved `default` profile, else the name. */
  display_name: string;
  /** Whether this is the live profile. */
  active: boolean;
  auto_match: ProfileAutoMatch | null;
  /** The display name of the profile's world, like `The Forsaken
   *  Lands`, or null when it has no world. */
  world_name: string | null;
  tracked_affects: TrackedAffect[];
  /** The pane tree the panel shows once this profile is live. */
  panes: PaneLayout;
  /** The live pane generation for the active profile, null for an
   *  inactive one. */
  generation: number | null;
  /** Whether the login toggle reads on. True only when the profile's
   *  first character logging in on its world would load this profile,
   *  so a profile that loses a claim to another reads off. */
  login_on: boolean;
}

export async function profileDetailGet(name: string): Promise<ProfileDetail> {
  const raw = await invoke<ProfileDetail>('profile_detail_get', { name });
  return {
    ...raw,
    tracked_affects: normalizeTrackedAffects(
      Array.isArray(raw.tracked_affects) ? raw.tracked_affects : [],
    ),
    panes: sanitizeLayout(raw.panes),
  };
}

/** A claim on the host alone that turning a login on pinned to its
 *  world's own port, so it keeps the character there. Every character
 *  it lists moved with it, since a profile holds one claim. */
export interface PinnedClaim {
  profile: string;
  port: number;
  characters: string[];
}

/** What turning a login toggle on or off did. */
export interface LoginClaim {
  /** The profile as it now reads. */
  entry: ProfileEntry;
  /** Every profile the character was taken from, in index order. */
  released_from: string[];
  /** Every claim pinned to its world's own port, in index order. */
  pinned: PinnedClaim[];
}

/** Turn the login toggle on or off for `character`. On takes the
 *  character from every other profile on the same world, since a
 *  character belongs to one profile per world. A claim on a known
 *  world's other port first pins a claim on the host alone to the
 *  world's own port, which keeps the character there. Off keeps the
 *  world and the name. Never switches a session. */
export async function profileSetLogin(
  name: string,
  character: string,
  on: boolean,
): Promise<LoginClaim> {
  return invoke('profile_set_login', { name, character, on });
}

/** Point a profile at a world. Edits only the host and port. A null or
 *  blank host clears the world. */
export async function profileSetWorld(
  name: string,
  host: string | null,
  port: number | null,
): Promise<ProfileEntry> {
  return invoke('profile_set_world', { name, host, port });
}

/** Where profileExportFile saved a profile. */
export interface ProfileExport {
  path: string;
  /** Like `Ilsabet profile.toml`. */
  file_name: string;
}

/** Save a profile's settings, active or not, as a TOML file in your
 *  Downloads folder. The name never replaces a file already there. The
 *  file names the profile's world and, of the characters it claims, the
 *  ones in `characters`. */
export async function profileExportFile(
  name: string,
  characters: string[] = [],
): Promise<ProfileExport> {
  return invoke('profile_export_file', { name, characters });
}

// Import. Characters reads a Vosh profile export you pick and imports it
// as a new profile or over one you have (Scripts Q9 and Q10).

/** A trigger or an alias in an export that runs Lua. */
export interface ImportLuaItem {
  kind: 'trigger' | 'alias';
  name: string;
}

/** A character the export names, and the profile of yours that has it
 *  on that world, raw like `default`, or null when none does. */
export interface ImportCharacter {
  name: string;
  claimed_by: string | null;
}

/** What a Vosh profile export holds and where it would go. */
export interface ImportPreview {
  /** The name the file name gives, or null when it breaks the profile
   *  name rule, so you type one. */
  name: string | null;
  triggers: number;
  aliases: number;
  macros: number;
  timers: number;
  /** Whether the file sets the tick its own way. */
  tick: boolean;
  variables: number;
  /** Pane type ids in tree order, like `map`. */
  panes: string[];
  runs_lua: ImportLuaItem[];
  /** The plugins the file turns on. Each comes in off. */
  plugins: string[];
  /** The world the file names, with its display name. */
  world: { host: string; port: number | null; name: string } | null;
  /** The characters the file names. Only a file with a world names any. */
  characters: ImportCharacter[];
  /** In loadout mode, whether the file holds presets, which stay out,
   *  since the catalog's presets serve every character (Presets Q11). */
  presets_stay: boolean;
}

/** Read `text`, the file you picked as `fileName`, as a Vosh profile
 *  export, and say what it holds. Changes nothing. A file that is no
 *  export is refused with the sentence to show. */
export async function profileImportRead(fileName: string, text: string): Promise<ImportPreview> {
  return invoke('profile_import_read', { fileName, text });
}

/** How an import adds the file. */
export type ImportAddAs = 'new' | 'replace';

/** What an import did. Profile names are raw, like `default`. */
export interface ImportResult {
  /** The profile the file went to. */
  name: string;
  /** Each character that left another profile for the new one, and
   *  whether that profile's login reads off now. */
  moved_from: { character: string; profile: string; login_off: boolean }[];
  /** Each character the file names that stays with the profile that
   *  has it. */
  kept_with: { character: string; profile: string }[];
  /** In loadout mode, the catalog group the file's triggers, aliases
   *  and macros joined. */
  catalog_group: string | null;
  /** The file's items the catalog already had, which stay yours. */
  clashes: { kind: 'trigger' | 'alias' | 'macro'; name: string }[];
}

/** Import `text`, the export you picked as `fileName`, as a new profile
 *  named `name` or over your profile `name`. A new profile takes each
 *  character in `logins`, from another profile if one has it. Replace
 *  keeps that profile's world, characters and plugins. */
export async function profileImportApply(
  fileName: string,
  text: string,
  addAs: ImportAddAs,
  name: string,
  logins: string[],
): Promise<ImportResult> {
  return invoke('profile_import_apply', { fileName, text, addAs, name, logins });
}

/** Who is logged in. */
export interface SessionIdentity {
  host: string;
  port: number;
  /** The character from Char.Status or Char.Name, once the MUD sends
   *  it. */
  character: string | null;
  /** The live profile. */
  profile: string;
  /** The profile that claims `character` on this world, or null when
   *  no profile does. A profile that only matches the host does not
   *  count. */
  claimed_by: string | null;
}

/** The current session identity, or null while no connection is up. */
export async function sessionIdentityGet(session?: number): Promise<SessionIdentity | null> {
  return (await invoke<SessionIdentity | null>('session_identity_get', { session })) ?? null;
}

/** Hear the session identity change after a connect, a disconnect, or
 *  a login. */
export async function subscribeSessionIdentity(
  cb: (identity: SessionIdentity | null) => void,
): Promise<UnlistenFn> {
  return listen<SessionIdentity | null>(SESSION_IDENTITY_CHANGED, (event) => {
    cb(event.payload ?? null);
  });
}

/** Hear an edit to any profile's detail, active or not, by name. */
export async function subscribeProfileChanged(cb: (name: string) => void): Promise<UnlistenFn> {
  return listen<unknown>(PROFILE_CHANGED, (event) => {
    const payload = event.payload as { name?: unknown } | null;
    if (typeof payload?.name === 'string') cb(payload.name);
  });
}
