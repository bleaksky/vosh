// The players a session snoops (Snoop SN2, SN3 and SN5), which
// src-tauri/src/session/snoop.rs keeps and stores/session/snoopStore.ts
// follows. The page reads every tab with its text the first time it
// shows a session, then hears session://snoop for the tab list and
// session://snoop-output for new text. Stop asks the game to end a snoop,
// Close drops an ended tab, and Open in a window moves the tabs into a
// window of their own. Each call acts on the session it names, or on the
// selected session when it names none.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { SNOOP, SNOOP_FIND, SNOOP_OUTPUT } from './events';
import { sessionOf } from './session';

/** One snooped player, `SnoopTab` in src-tauri/src/session/snoop.rs. A
 *  snoop you pressed Stop on reads live until the game ends it. Times
 *  are ms since the epoch. */
export interface SnoopTab {
  name: string;
  live: boolean;
  ended_at: number | null;
  last_output_at: number | null;
}

/** Every tab of a session in the order they started, and whether they
 *  show in the snoop window, with the split closed meanwhile. */
export interface SnoopList {
  tabs: SnoopTab[];
  windowed: boolean;
}

/** What `snoop_get` answers: the list, each tab with its text as the
 *  game sent it, ANSI and all, up to 5,000 lines. */
export interface SnoopSnapshot {
  tabs: (SnoopTab & { text: string })[];
  windowed: boolean;
}

/** The text one player's screen got in one read, raw. */
export interface SnoopOutput {
  name: string;
  text: string;
}

/** Every tab `session` holds, with its text. */
export function snoopGet(session?: number): Promise<SnoopSnapshot> {
  return invoke<SnoopSnapshot>('snoop_get', { session });
}

/** Send the game `snoop stop` with `name`, or alone to stop every
 *  snoop. The tab goes once the game says the snoop ended. */
export function snoopStop(session?: number, name?: string): Promise<void> {
  return invoke<void>('snoop_stop', { session, name });
}

/** Close the ended tab of `name`, or every ended tab, with its text. */
export function snoopClose(session?: number, name?: string): Promise<void> {
  return invoke<void>('snoop_close', { session, name });
}

/** Move every tab of `session` into its snoop window, opening it or
 *  bringing it forward. Closing it hands the tabs back to the split. */
export function snoopWindowOpen(session?: number): Promise<void> {
  return invoke<void>('snoop_window_open', { session });
}

/** Hear a session's tab list each time it changes, with that session. */
export function onSnoop(cb: (list: SnoopList, session: number) => void): Promise<UnlistenFn> {
  return listen<SnoopList & { session?: number }>(SNOOP, (event) => {
    const { tabs, windowed } = event.payload;
    cb({ tabs, windowed }, sessionOf(event.payload));
  });
}

/** Hear the new text of one snooped player, with its session. */
export function onSnoopOutput(
  cb: (output: SnoopOutput, session: number) => void,
): Promise<UnlistenFn> {
  return listen<SnoopOutput & { session?: number }>(SNOOP_OUTPUT, (event) => {
    const { name, text } = event.payload;
    cb({ name, text }, sessionOf(event.payload));
  });
}

/** Hear Find in the menu bar, chosen while a snoop window is in front,
 *  with the session of that window. */
export function onSnoopFind(cb: (session: number) => void): Promise<UnlistenFn> {
  return listen<number>(SNOOP_FIND, (event) => cb(event.payload));
}
