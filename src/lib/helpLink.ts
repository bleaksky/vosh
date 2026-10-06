import { emitHelpGoto, openHelpWindow } from '../ipc/windows';
import { rankTopics, resolveHelpTarget } from '../help/helpNav';

// Opening Help from another window, the twin of settingsLink.ts. The
// Help window may not exist yet, so a target travels twice: through
// localStorage for a cold open and through an event for a window that
// is already up. HelpWindow reads both and resolves the string with
// resolveHelpTarget (src/help/helpNav.ts): a topic id, a topic number,
// or words to search for.

/** Where a cold open finds its target. */
export const HELP_PENDING_KEY = 'vosh.help.pending';

/** Open Help on `target`, a topic id like `shape.prompt-show`, a topic
 *  number like `9.3`, or words to search for. */
export function openHelpTopic(target: string): void {
  try {
    localStorage.setItem(HELP_PENDING_KEY, target);
  } catch {
    // Storage unavailable. The event still reaches an open window.
  }
  void emitHelpGoto(target);
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
