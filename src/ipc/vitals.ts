// Your vitals for a window that opens between two Char.Vitals packets,
// and the vitals text a footer or the status line draws.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { VITALS_TEXT } from './events';
import type { PromptRendered } from './promptDesign';
import { sessionOf } from './session';

/** The last Char.Vitals and Char.Combat of a session's connection, raw
 *  as the game sent them, each null before the first one and after the
 *  connection ends, and the last 60 Char.Vitals the game showed, oldest
 *  first, each with the time it came in ms since the epoch. Mirrors
 *  VitalsSnapshot in src-tauri/src/ipc/vitals.rs. */
export interface VitalsSnapshot {
  vitals: unknown;
  combat: unknown;
  history?: { at: number; vitals: unknown }[];
}

/** Vosh's vitals text, which a profile that sets no vitals_text draws.
 *  Mirrors DEFAULT_VITALS_TEXT in crates/prompt/src/config.rs, and both
 *  are held to fixtures/ui-config/vitals-text.txt. */
export const VOSH_VITALS_TEXT =
  '%{if:fight}%opponent%{right}%c_yellow%{opponent_hp:pct}%%%c_default%nl%{end}' +
  '%{c:hp:game}%hp%c_gray/%{maxhp}hp%c_default ' +
  '%mana%c_gray/%{maxmana}mn%c_default ' +
  '%move%c_gray/%{maxmove}mv%c_default';

/** The text Text starts from while you have none. Your 0.7 template,
 *  in today's codes, while it was on (Vitals Styles Q13), or Vosh's. */
export function startVitalsText(legacy: string | null): string {
  return legacy ?? VOSH_VITALS_TEXT;
}

/** The text the Text style draws, yours or the one it starts from.
 *  Mirrors UiConfig::vitals_text_drawn in src-tauri/src/profile/ui.rs. */
export function drawnVitalsText(config: {
  vitals_text: string;
  vitals_legacy_text: string | null;
}): string {
  return config.vitals_text || startVitalsText(config.vitals_legacy_text);
}

/** Read the last vitals and fight of a session, the selected one when it
 *  names none, so the gallery in Settings draws your numbers as it
 *  opens (Vitals Styles Q17). */
export async function vitalsSnapshotGet(session?: number): Promise<VitalsSnapshot> {
  return invoke('vitals_snapshot_get', { session });
}

/** Tell a session that a footer or the status line draws your vitals
 *  text `cols` cells wide, or that nothing draws it with null. While a
 *  watch is on, the session sends the text at once and whenever it
 *  moves. */
export async function vitalsTextWatch(cols: number | null, session?: number): Promise<void> {
  await invoke('vitals_text_watch', { cols, session });
}

/** One render of your vitals text, on session://vitals-text. Mirrors
 *  VitalsText in crates/prompt/src/vitals.rs. */
export interface VitalsText {
  /** The session that drew it. */
  session: number;
  /** The text at the live values. */
  live: PromptRendered;
  /** The same text with every vital at its max and your opponent at
   *  100, so the footer can keep the room the text takes at full. */
  full: PromptRendered;
  /** For each live row, whether it reads your fight. */
  fight: boolean[];
  /** The pieces that are a `%{right}`, so the footer finds in the spans
   *  where a row pushes and keeps what follows whole. */
  right: number[];
}

/** Hear each render of your vitals text, with the session that drew it. */
export async function onVitalsText(cb: (text: VitalsText) => void): Promise<UnlistenFn> {
  return listen<VitalsText>(VITALS_TEXT, (event) => {
    cb({ ...event.payload, session: sessionOf(event.payload) });
  });
}
