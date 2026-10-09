// The alerts a session rings, from a trigger's alert, an alert preset or
// mud.alert in Lua.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AlertParts } from './automation';
import { ALERT, ALERTS_ENDED, MARK } from './events';
import { sessionOf } from './session';

/** An alert that rang, on session://alert. Mirrors AlertPayload in
 *  src-tauri/src/alert.rs. */
export interface AlertPayload {
  /** The session that rang it, as every session event names it. */
  session: number;
  /** The banner's title, such as `Tell from Tolliver`. */
  title: string;
  /** The session as its row reads, which the banner names under the
   *  title. */
  label: string | null;
  /** What was said or the line that matched, with Title and words on. */
  words: string | null;
  /** The tone the page plays, or null when the alert has none or Vosh
   *  played a system sound in its place. */
  sound: string | null;
  /** A system banner went out. */
  banner: boolean;
  /** Vosh is in front and you look at another session, so the page
   *  shows a notice of its own in place of a banner. */
  notice: boolean;
  /** Where it came from, `trigger:<name>`, `preset:<id>` or
   *  `lua:<owner>`. */
  source: string;
  /** The owner tag of the Lua that raised it, such as
   *  `plugin:vitals_alert`. */
  owner: string | null;
}

/** Hear each alert that rings, with the session it rang in. The main
 *  window plays its tone, and the sessions sidebar marks the session's
 *  row. */
export async function onAlert(cb: (alert: AlertPayload) => void): Promise<UnlistenFn> {
  return listen<AlertPayload>(ALERT, (event) => {
    cb({ ...event.payload, session: sessionOf(event.payload) });
  });
}

/** Hear each alert that rang nothing in a session behind, an alert
 *  preset that is off or an alert held back, with that session and where
 *  the alert came from, as `onAlert` gives it. One comes for each such
 *  alert. The sessions sidebar marks the row as it does for an alert
 *  that rang. */
export async function onMark(cb: (session: number, source: string) => void): Promise<UnlistenFn> {
  return listen<{ session?: number; source: string }>(MARK, (event) => {
    cb(sessionOf(event.payload), event.payload.source);
  });
}

/** Hear the alerts of a Lua owner end in a session, with that session
 *  and the owner tag, such as `plugin:watch`. Mirrors AlertsEnded in
 *  src-tauri/src/alert.rs: a plugin was turned off, stopped or reloaded,
 *  so the page drops the notice it raised. */
export async function onAlertsEnded(
  cb: (session: number, owner: string) => void,
): Promise<UnlistenFn> {
  return listen<{ session?: number; owner: string }>(ALERTS_ENDED, (event) => {
    cb(sessionOf(event.payload), event.payload.owner);
  });
}

/** What the alert presets of the selected session's profile do, from
 *  alert_presets_get. Mirrors AlertPresets in src-tauri/src/ipc/alerts.rs. */
export interface AlertPresets {
  /** What each preset does, by id, the profile's `[alerts]` table. A
   *  preset it leaves out posts a banner alone. */
  alerts: Record<string, AlertParts>;
  /** The five ids, in the order the Alerts category lists them. */
  ids: string[];
  /** The ids that ring. Rust reads them off enabled_presets, where the
   *  marker that turns every preset off wins over the list. */
  on: string[];
}

export async function alertPresetsGet(profile?: string | null): Promise<AlertPresets> {
  return invoke<AlertPresets>('alert_presets_get', { profile });
}

/** Set what the preset `id` does, or with null, forget its parts so it
 *  posts a banner alone. Rust saves the profile at once. Whether it
 *  rings stays with enabled_presets. */
export async function alertPresetsSet(
  id: string,
  alert: AlertParts | null,
  profile?: string | null,
): Promise<void> {
  await invoke('alert_presets_set', { id, alert, profile });
}

/** Whether the system lets Vosh post banners, as Rust serializes
 *  Permission in src-tauri/src/alert/banner.rs. `unavailable` is a build
 *  that cannot post them, such as a dev build on macOS. */
export type Permission = 'granted' | 'denied' | 'not_asked' | 'unavailable';

export async function alertsPermission(): Promise<Permission> {
  return invoke<Permission>('alerts_permission');
}

/** Ask the system to let Vosh post banners. macOS shows its own question
 *  the first time, and this answers once you choose. */
export async function alertsAskPermission(): Promise<Permission> {
  return invoke<Permission>('alerts_ask_permission');
}

/** Open the system's notification settings, at Vosh where it can. */
export async function alertsOpenSettings(): Promise<void> {
  await invoke('alerts_open_settings');
}
