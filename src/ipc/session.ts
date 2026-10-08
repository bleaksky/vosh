// Calls and events of your sessions and their connections. List and
// select the sessions, connect, send a line, hear the state, the GMCP
// packages, routed text and your target, and tell every window where
// Connect dials.

import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  CONNECTION_TARGET_CHANGED,
  INPUT_MODE,
  RECONNECT,
  ROUTED,
  SESSION_SELECTED,
  SESSIONS_CHANGED,
  STATE,
  TARGET,
  WALK,
} from './events';

/** The session the app starts with, `SessionId::FIRST` in
 *  src-tauri/src/sessions.rs. */
export const FIRST_SESSION = 1;

/** The session an event's payload names. Every session event the app
 *  sends names its own. A payload that names none reads as the first
 *  session's, as every event did before the page told sessions apart. */
export function sessionOf(payload: { session?: unknown }): number {
  return typeof payload.session === 'number' ? payload.session : FIRST_SESSION;
}

/** One session as the sidebar lists it, `SessionRow` in
 *  src-tauri/src/sessions.rs. */
export interface SessionRow {
  id: number;
  /** The name you gave it. */
  name: string | null;
  /** The character it plays, or once the link is gone the one it
   *  played last, which a connect you start forgets until you log in. */
  character: string | null;
  /** Where it dials, null before its first connect or address. */
  host: string | null;
  port: number | null;
  tls: boolean;
  /** The profile it plays, null only before launch loads one. */
  profile: string | null;
  connected: boolean;
  /** When the live link reached the game, in Unix ms, for the time
   *  online. Null while no link runs. */
  since: number | null;
  selected: boolean;
}

/** Every session in the order the sidebar lists them, the selected one
 *  marked. */
export async function listSessions(): Promise<SessionRow[]> {
  return invoke('sessions_list');
}

/** Select a session. The commands that name no session act on it from
 *  then on, and every window hears the rows again. */
export async function selectSession(session: number): Promise<void> {
  await invoke('session_select', { session });
}

/** Open a session after the others, with nothing connected, playing
 *  `profile`, and answer its id. Its Lua engine starts on the profile
 *  as it opens. */
export async function openSession(profile: string): Promise<number> {
  return invoke('session_open', { profile });
}

/** Close a session, ending its connection. A selected session hands the
 *  selection on. */
export async function closeSession(session: number): Promise<void> {
  await invoke('session_close', { session });
}

/** Give a session the name its row, the title band and the window title
 *  read in place of its character. With none, or a blank one, it reads
 *  its character again. The app keeps the name for the next launch and
 *  sends every window the rows. */
export async function renameSession(session: number, name: string | null): Promise<void> {
  await invoke('session_rename', { session, name });
}

/** Move a session to the place `to` among the other rows, counting from
 *  0, as a drag of its row does. The app keeps the order for the next
 *  launch and sends every window the rows. */
export async function moveSession(session: number, to: number): Promise<void> {
  await invoke('session_move', { session, to });
}

/** Hear every session's row after a step that changed what one shows. */
export async function onSessionsChanged(cb: (rows: SessionRow[]) => void): Promise<UnlistenFn> {
  return listen<SessionRow[]>(SESSIONS_CHANGED, (event) => {
    cb(event.payload);
  });
}

/** Hear Vosh select a session itself, as a click on an alert banner
 *  does. */
export async function onSessionSelected(cb: (session: number) => void): Promise<UnlistenFn> {
  return listen<{ session: number }>(SESSION_SELECTED, (event) => {
    cb(event.payload.session);
  });
}

/** Where a session's connection stands, with the session it is. */
export type StatePayload = { session: number } & (
  | { kind: 'connecting'; host: string; port: number; tls: boolean }
  | { kind: 'connected'; host: string; port: number; tls: boolean }
  | { kind: 'disconnected'; reason: string | null }
);

/** Where the redial of a session stands after a drop,
 *  `ReconnectPayload` in src-tauri/src/session/reconnect.rs. A series
 *  waits and dials each try in turn until one reaches the game or the
 *  tries run out. Your Disconnect, a Connect or a close cancels it. A
 *  drop declines to redial for a quit of yours, the game's closing line,
 *  another session that took the character, or Reconnect when the link
 *  drops turned off. */
export type ReconnectPayload =
  | { kind: 'waiting'; try: number; tries: number; seconds: number }
  | { kind: 'dialing'; try: number; tries: number }
  | { kind: 'failed'; try: number; tries: number; reason: string }
  | { kind: 'reached'; try: number }
  | { kind: 'stopped'; tries: number }
  | { kind: 'cancelled' }
  | { kind: 'declined'; why: 'quit' | 'closing' | 'taken' | 'off' };

/** Hear each step of a session's redial, with that session. */
export async function onReconnect(
  cb: (payload: ReconnectPayload, session: number) => void,
): Promise<UnlistenFn> {
  return listen<ReconnectPayload & { session?: number }>(RECONNECT, (event) => {
    cb(event.payload, sessionOf(event.payload));
  });
}

/** Where Connect dials. */
export interface ConnectionTarget {
  host: string;
  port: number;
  tls: boolean;
}

/** Keep where a session dials, without dialing. Its row names that
 *  world from then on. */
export async function setSessionAddress(session: number, target: ConnectionTarget): Promise<void> {
  await invoke('session_set_address', {
    session,
    host: target.host,
    port: target.port,
    tls: target.tls,
  });
}

/** Dial `host` in a session, the selected one when it names none. */
export async function connectSession(
  host: string,
  port: number,
  tls: boolean,
  session?: number,
): Promise<void> {
  await invoke('session_connect', { host, port, tls, session });
}

/** End a session's connection, the selected one's when it names
 *  none. */
export async function disconnectSession(session?: number): Promise<void> {
  await invoke('session_disconnect', { session });
}

/** Tell every window, this one included, the Connect target you saved. */
export function emitConnectionTargetChanged(target: ConnectionTarget): Promise<void> {
  return emit(CONNECTION_TARGET_CHANGED, target);
}

/** Hear the Connect target any window saved. Pages follow it through
 *  subscribeConnectionTarget in useConnection.ts, which parses it first. */
export function subscribeConnectionTargetChanged(
  cb: (payload: unknown) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(CONNECTION_TARGET_CHANGED, (event) => cb(event.payload));
}

/** Push the live terminal size to the backend. The backend updates
 *  the telnet negotiator and emits a NAWS subnegotiation when NAWS
 *  has already been agreed with the server. MUDs that honor NAWS
 *  then re-wrap their output at the new column count, which is what
 *  word-wrap actually looks like: server-side wrapping at word
 *  boundaries instead of mid-character. No-op when not connected. */
export async function setWindowSize(cols: number, rows: number, session?: number): Promise<void> {
  await invoke('session_set_window_size', { cols, rows, session });
}

/// Run a typed input line through the backend pipeline of a session, the
/// selected one when it names none. Variables, aliases, and slash commands
/// are handled there; the result either goes to the connection or echoes
/// back as a session://output event.
export async function sendInput(line: string, session?: number): Promise<void> {
  await invoke('session_send_input', { line, session });
}

/// Send a line typed into the masked password field to a session, the
/// selected one when it names none. It goes to the server exactly as
/// typed, past aliases, variables, and slash commands, and the session
/// log keeps `> (hidden)` in its place.
export async function sendMaskedInput(line: string, session?: number): Promise<void> {
  await invoke('session_send_masked', { line, session });
}

/// Stop the walk under way in a session, the selected one when it names
/// none, as Esc in the command line does. The session says nothing when
/// you are not walking.
export async function stopWalk(session?: number): Promise<void> {
  await invoke('session_walk_stop', { session });
}

/** Where a walk stands, `WalkProgress` in src-tauri/src/session/walk.rs.
 *  `left` is the steps still to go as a `#walk` string, and `route` is
 *  true for a walk a click on the map started. */
export type WalkProgress =
  | { kind: 'idle' }
  | { kind: 'walking'; done: number; total: number; left: string; route: boolean }
  | {
      kind: 'stopped';
      done: number;
      total: number;
      why: 'plain' | 'lost_sight' | 'lost_track';
    };

/** Hear each change to where a session's walk stands, with that
 *  session. A walk ends idle when it arrives or the connection drops. */
export async function onWalk(
  cb: (progress: WalkProgress, session: number) => void,
): Promise<UnlistenFn> {
  return listen<WalkProgress & { session?: number }>(WALK, (event) => {
    cb(event.payload, sessionOf(event.payload));
  });
}

/** What a GMCP package, the prompt values and the affect fulls carry,
 *  the session that sent them beside their data. A `session` key
 *  among the data's own would read as one more field, value or
 *  affect. */
export interface SessionData<T> {
  session: number;
  data: T;
}

// Hear one GMCP package. The session sends each package on an event of
// its own, `session://gmcp/<package>`, so a listener runs only on the
// packets it reads. A store that reads several, such as the room, chat
// and group stores, calls this once for each and keeps each unlisten.
//
// Tauri event names allow only letters, digits and `-/:_`, so the dots
// of a package name (`Char.Vitals`) become dashes (`Char-Vitals`), as
// the session sends them. Callers pass the name with its dots.
//
// The payload is a SessionData, and the callback gets its data, the
// packet as the game sent it, in the type the caller names as the
// generic, and the session that sent it.
//
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function onGmcpPackage<T = any>(
  name: string,
  cb: (data: T, session: number) => void,
): Promise<UnlistenFn> {
  const channel = `session://gmcp/${name.replace(/\./g, '-')}`;
  return listen<SessionData<T>>(channel, (event) => {
    cb(event.payload.data, sessionOf(event.payload));
  });
}

export interface RoutedPayload {
  pane: string;
  text: string;
}

/** Hear each line a trigger routes to a pane, with the session that
 *  printed it. */
export async function onRouted(
  cb: (payload: RoutedPayload, session: number) => void,
): Promise<UnlistenFn> {
  return listen<RoutedPayload & { session?: number }>(ROUTED, (event) => {
    cb(event.payload, sessionOf(event.payload));
  });
}

export interface QuickKey {
  name: string;
  verb: string;
}

export interface TargetPayload {
  name: string | null;
  /// 1-based position in the latest Room.Chars push that the
  /// backend resolved as the targeted char. `null` when the target
  /// isn't in the current room or no target is set.
  room_idx: number | null;
  /// Current quick-key bindings (name → verb). Includes empty-verb
  /// entries; the TargetBar filters them for display.
  quick_keys: QuickKey[];
}

/** A session's target and quick keys, the selected session's when it
 *  names none. */
export async function getTarget(session?: number): Promise<TargetPayload> {
  return invoke('target_get', { session });
}

/** Hear each change to a session's target or quick keys, with that
 *  session. */
export async function onTarget(
  cb: (payload: TargetPayload, session: number) => void,
): Promise<UnlistenFn> {
  return listen<TargetPayload & { session?: number }>(TARGET, (event) => {
    const { name, room_idx, quick_keys } = event.payload;
    cb({ name, room_idx, quick_keys }, sessionOf(event.payload));
  });
}

export async function onState(cb: (state: StatePayload) => void): Promise<UnlistenFn> {
  return listen<StatePayload>(STATE, (event) => {
    cb({ ...event.payload, session: sessionOf(event.payload) });
  });
}

export interface InputModePayload {
  password: boolean;
}

/** Hear the game turn its echo off or on in a session, with that
 *  session. */
export async function onInputMode(
  cb: (payload: InputModePayload, session: number) => void,
): Promise<UnlistenFn> {
  return listen<InputModePayload & { session?: number }>(INPUT_MODE, (event) => {
    cb({ password: event.payload.password }, sessionOf(event.payload));
  });
}
