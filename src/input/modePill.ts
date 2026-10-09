import type { WalkProgress } from '../ipc/session';
import type { WritingState } from '../ipc/writing';
import { editorPill } from '../writing/kinds';
import { editorLineOf } from './editorLine';

// The pill at the start of the command line, in place of the mark, while
// something changes what Enter does: a password prompt, the game's line
// editor on a text Vosh names, the game's pager, or a walk. One shows at
// a time, in that order.

export type Mode = 'password' | 'editor' | 'pager' | 'walk';

export interface ModePill {
  mode: Mode;
  /** What the pill names, like Description. */
  name: string;
  /** What follows the name, like 4 of 30, or null for the name alone. */
  count: string | null;
  /** The editor holds more lines than its help allows. */
  warn: boolean;
  /** The line's placeholder while the pill shows, or null for none. */
  hint: string | null;
  /** The pill's accessible name. */
  label: string;
}

const EDITOR_HINT = 'Type @ on a blank line to finish';

const steps = (n: number) => (n === 1 ? '1 step left' : `${n} steps left`);

/** The pill the line shows now, or null for the mark. */
export function modeOf({
  password,
  writing,
  walk,
}: {
  password: boolean;
  writing: WritingState;
  walk: WalkProgress;
}): ModePill | null {
  if (password) {
    return {
      mode: 'password',
      name: 'Password',
      count: null,
      warn: false,
      hint: null,
      label: 'Password',
    };
  }
  const editor = editorLineOf(writing);
  if (editor && writing.game !== 'pager') {
    const { pill, maxLines } = editorPill(editor.kind);
    // The line you are on is the one after those the editor holds.
    const at = writing.lines === null ? null : writing.lines + 1;
    const count = at === null ? null : maxLines === null ? `line ${at}` : `${at} of ${maxLines}`;
    return {
      mode: 'editor',
      name: pill,
      count,
      warn: at !== null && maxLines !== null && at > maxLines,
      hint: EDITOR_HINT,
      label: count === null ? pill : `${pill}, ${maxLines === null ? count : `line ${count}`}`,
    };
  }
  if (writing.game === 'pager' && writing.job === null) {
    return {
      mode: 'pager',
      name: 'More',
      count: null,
      warn: false,
      hint: 'Press Enter for the next page',
      label: 'More',
    };
  }
  if (walk.kind === 'walking') {
    const left = steps(walk.total - walk.done);
    return {
      mode: 'walk',
      name: 'Walking',
      count: left,
      warn: false,
      hint: 'Esc stops the walk',
      label: `Walking, ${left}`,
    };
  }
  return null;
}
