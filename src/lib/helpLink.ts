import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { rankTopics, resolveHelpTarget } from './helpNav';

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
/** `#help <words>` reaches the main window here, with the words. */
export const HELP_OPEN_EVENT = 'vosh://help-open';
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

/** What the terminal says when `#help <words>` finds no topic. */
export function helpNoMatchNotice(words: string): string {
  return `No help topic mentions ${words}. Type #help for the slash commands.`;
}

/** Whether `#help <words>` has a topic to open: a topic id or number,
 *  or words some topic holds. */
export function helpOpensOn(words: string): boolean {
  const target = resolveHelpTarget(words);
  if (!target) return false;
  return target.kind === 'topic' || rankTopics(target.query).length > 0;
}
