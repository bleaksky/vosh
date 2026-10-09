// The words Vosh knows on the command line, read from the backend so the
// coloring as you type never guesses what Vosh would run.

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
