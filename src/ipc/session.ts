// Calls and events of the live connection. Connect, send a line, hear
// the state, the GMCP packages, routed text and your target, and tell
// every window where Connect dials.

import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { ConnectionTarget } from '../lib/useConnection';
import { CONNECTION_TARGET_CHANGED, INPUT_MODE, ROUTED, STATE, TARGET } from './events';

export type StatePayload =
  | { kind: 'connecting'; host: string; port: number; tls: boolean }
  | { kind: 'connected'; host: string; port: number; tls: boolean }
  | { kind: 'disconnected'; reason: string | null };

export async function connectSession(host: string, port: number, tls: boolean): Promise<void> {
  await invoke('session_connect', { host, port, tls });
}

export async function disconnectSession(): Promise<void> {
  await invoke('session_disconnect');
}

/** Tell every window, this one included, the Connect target you saved. */
export function emitConnectionTargetChanged(target: ConnectionTarget): Promise<void> {
  return emit(CONNECTION_TARGET_CHANGED, target);
}

/** Hear the Connect target any window saved. Pages follow it through
 *  subscribeConnectionTarget in lib/useConnection.ts, which parses it
 *  first. */
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
export async function setWindowSize(cols: number, rows: number): Promise<void> {
  await invoke('session_set_window_size', { cols, rows });
}

/// Run a typed input line through the backend pipeline. Variables, aliases,
/// and slash commands are handled there; the result either goes to the
/// connection or echoes back as a session://output event.
export async function sendInput(line: string): Promise<void> {
  await invoke('session_send_input', { line });
}

/// Send a line typed into the masked password field. It goes to the
/// server exactly as typed, past aliases, variables, and slash commands,
/// and the session log keeps `> (hidden)` in its place.
export async function sendMaskedInput(line: string): Promise<void> {
  await invoke('session_send_masked', { line });
}

/// Stop the walk under way, as Esc in the command line does. The session
/// says nothing when you are not walking.
export async function stopWalk(): Promise<void> {
  await invoke('session_walk_stop');
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
// generic.
//
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function onGmcpPackage<T = any>(
  name: string,
  cb: (data: T) => void,
): Promise<UnlistenFn> {
  const channel = `session://gmcp/${name.replace(/\./g, '-')}`;
  return listen<SessionData<T>>(channel, (event) => {
    cb(event.payload.data);
  });
}

export interface RoutedPayload {
  pane: string;
  text: string;
}

export async function onRouted(cb: (payload: RoutedPayload) => void): Promise<UnlistenFn> {
  return listen<RoutedPayload>(ROUTED, (event) => {
    cb(event.payload);
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

export async function getTarget(): Promise<TargetPayload> {
  return invoke('target_get');
}

export async function onTarget(cb: (payload: TargetPayload) => void): Promise<UnlistenFn> {
  return listen<TargetPayload>(TARGET, (event) => {
    cb(event.payload);
  });
}

export async function onState(cb: (state: StatePayload) => void): Promise<UnlistenFn> {
  return listen<StatePayload>(STATE, (event) => {
    cb(event.payload);
  });
}

export interface InputModePayload {
  password: boolean;
}

export async function onInputMode(cb: (payload: InputModePayload) => void): Promise<UnlistenFn> {
  return listen<InputModePayload>(INPUT_MODE, (event) => {
    cb(event.payload);
  });
}
