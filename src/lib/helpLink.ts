import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';

// Opening Help from another window, the twin of settingsLink.ts. The
// Help window may not exist yet, so a target travels twice: through
// localStorage for a cold open and through an event for a window that
// is already up. HelpApp reads both and resolves the string with
// resolveHelpTarget (src/lib/helpNav.ts): a topic id, a topic number,
// or words to search for.

/** Where a cold open finds its target. */
export const HELP_PENDING_KEY = 'vosh.help.pending';
/** The event an open Help window listens on. */
export const HELP_GOTO_EVENT = 'vosh://help-goto';
/** Find, chosen in the menu bar while Help is in front. */
export const HELP_FIND_EVENT = 'vosh://help-find';

/** Open Help, or bring it forward, where it is. */
export function openHelpWindow(): void {
  invoke('open_help_window').catch((e: unknown) => {
    console.error('[help] open_help_window failed', e);
  });
}

/** Open Help on `target`, a topic id like `shape.prompt-show`, a topic
 *  number like `9.3`, or words to search for. */
export function openHelpTopic(target: string): void {
  try {
    localStorage.setItem(HELP_PENDING_KEY, target);
  } catch {
    // Storage unavailable. The event still reaches an open window.
  }
  void emit(HELP_GOTO_EVENT, target);
  openHelpWindow();
}
