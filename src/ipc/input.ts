// The command line's calls into Rust. The words Vosh knows, read from the
// backend so the coloring as you type never guesses what Vosh would run,
// and the history each session keeps across a restart.

import { invoke } from '@tauri-apps/api/core';

/** The first words Vosh acts on in a session. */
export interface KnownWords {
  /** The aliases that expand if you press Enter now, sorted. */
  aliases: string[];
  /** The # commands Vosh runs, each without its #. */
  commands: string[];
}

/** The aliases and # commands Vosh knows in `session`. */
export async function inputKnownWords(session: number): Promise<KnownWords> {
  return invoke<KnownWords>('input_known_words', { session });
}

/** The commands `session` sent in earlier runs, oldest first. */
export async function historyGet(session: number): Promise<string[]> {
  return invoke<string[]>('history_get', { session });
}

/** Keep `line`, a command `session` sent, for the next run. False for a
 *  line that gives the game a password, which history never keeps. */
export async function historyPush(line: string, session: number): Promise<boolean> {
  return invoke<boolean>('history_push', { line, session });
}
