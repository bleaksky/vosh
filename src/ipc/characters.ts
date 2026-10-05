// One profile as Settings shows it under Characters, its login claim and
// world, its export, and who is logged in.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { sanitizeLayout, type PaneLayout } from '../lib/paneLayout';
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

/** Put a profile's panes back to the stock map over affects tree,
 *  keeping whether its panel shows and how wide it is. Returns the new
 *  layout. The live profile saves it at once and every window hears it
 *  through vosh://pane-layout-changed. */
export async function paneLayoutReset(profile?: string | null): Promise<PaneLayout> {
  return sanitizeLayout(await invoke<unknown>('pane_layout_reset', { profile: profile ?? null }));
}

/** What turning a login toggle on or off did. */
export interface LoginClaim {
  /** The profile as it now reads. */
  entry: ProfileEntry;
  /** Every profile the character was taken from, in index order. */
  released_from: string[];
}

/** Turn the login toggle on or off for `character`. On takes the
 *  character from every other profile on the same world, since a
 *  character belongs to one profile per world. Off keeps the world and
 *  the name. Never switches the live profile. */
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
 *  Downloads folder. The name never replaces a file already there. */
export async function profileExportFile(name: string): Promise<ProfileExport> {
  return invoke('profile_export_file', { name });
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
export async function sessionIdentityGet(): Promise<SessionIdentity | null> {
  return (await invoke<SessionIdentity | null>('session_identity_get')) ?? null;
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
