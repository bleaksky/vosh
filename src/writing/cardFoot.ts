import type { JobProgress, WritingKind } from '../ipc/writing';
import { KINDS } from './kinds';
import { progressLine, type Note } from './words';

// What the writing card's footer shows (Description Editor board 1, Note
// Editor boards 1 and 4). On the left, in this order: a job's progress,
// what the last job left, what a paste changed, a text past the game's
// room, a run the game takes as spam, the caret's line, a game busy in
// another editor, and the count. On the right, the fix for what the left
// says beside the card's main button.

/** What the last job left in the footer until you change the text, and
 *  the buttons it brings. */
export interface Ended {
  note: Note;
  /** `done` ends the card where the game said no for good, such as an
   *  application it turned down (Note Editor board 5). */
  actions: ('restore' | 'again' | 'clear-other' | 'done')[];
  /** The board whose note `clear-other` clears. */
  other?: WritingKind | null;
}

export type FootLeft =
  | { progress: string }
  | { note: Note }
  | { count: { main: string; tone: 'n' | 'warn' | 'bad'; rest: string } };

export type FootAction =
  | 'stop'
  | 'undo'
  | 'rewrap'
  | 'clear-other'
  | 'restore'
  | 'done'
  | 'post'
  | 'check'
  | 'send';

export interface FootButton {
  id: FootAction;
  label: string;
  primary?: boolean;
  disabled?: boolean;
}

export interface FootInput {
  kind: WritingKind;
  running: JobProgress | null;
  ended: Ended | null;
  paste: Note | null;
  /** Characters past the game's room, or null when the text fits. */
  over: number | null;
  /** The first row of a run the game takes as spam. */
  spam: number | null;
  /** What is wrong with the caret's line, and whether Rewrap fixes it. */
  flagged: { note: Note; rewrap: boolean } | null;
  /** The game waits in a line editor the card did not open. */
  busy: boolean;
  count: { main: string; tone: 'n' | 'warn' | 'bad'; rest: string };
  live: boolean;
  phase: 'edit' | 'sent' | 'posted' | 'checked';
  /** A post opened from Sent, read only. */
  sentView: boolean;
  canSend: boolean;
  canPost: boolean;
  /** The game holds the text as the card shows it, read or sent. */
  matches: boolean;
  /** The card knows the game's copy, which Restore sends back. */
  hasGame: boolean;
}

export function footFor(f: FootInput): { left: FootLeft; buttons: FootButton[] } {
  const info = KINDS[f.kind];
  let left: FootLeft = {
    count: { ...f.count, rest: f.count.rest + (f.live ? '' : ' · Not connected') },
  };
  if (f.running) left = { progress: progressLine(f.running) };
  else if (f.ended) left = { note: f.ended.note };
  else if (f.paste) left = { note: f.paste };
  else if (f.over !== null)
    left = {
      note: {
        lead: `This is ${f.over.toLocaleString('en-US')} characters too long for the game.`,
        rest: info.board ? ' Cut it down or split it in two.' : ' Cut it down to send it.',
        tone: 'bad',
      },
    };
  else if (f.spam !== null)
    left = {
      note: {
        lead: `Lines ${f.spam + 1} to ${f.spam + 26} are the same line.`,
        rest: ' The game takes that as spam, so change one of them.',
        tone: 'warn',
      },
    };
  else if (f.flagged) left = { note: f.flagged.note };
  else if (f.busy)
    left = {
      note: info.board
        ? {
            lead: 'You’re in another editor in the game.',
            rest: ' Type @ to finish it, then post.',
            tone: 'warn',
          }
        : {
            lead: 'The game is waiting in a line editor.',
            rest: ' End it with @, then send.',
            tone: 'warn',
          },
    };

  if (f.running) {
    return {
      left,
      buttons: [{ id: 'stop', label: 'Stop', disabled: f.running.stage === 'posting' }],
    };
  }
  const buttons: FootButton[] = [];
  if (f.paste && !f.ended) buttons.push({ id: 'undo', label: 'Undo' });
  if (f.flagged?.rewrap && !f.ended && !f.paste)
    buttons.push({ id: 'rewrap', label: 'Rewrap paragraph' });
  if (f.ended?.actions.includes('clear-other') && f.ended.other)
    buttons.push({ id: 'clear-other', label: 'Clear it…' });
  if (f.ended?.actions.includes('restore') && f.hasGame)
    buttons.push({ id: 'restore', label: 'Restore the game’s copy' });
  const again = f.ended?.actions.includes('again') ?? false;
  const checkLabel = f.kind === 'description' ? 'Send for approval…' : 'Send for review…';
  if (
    f.phase === 'posted' ||
    f.phase === 'checked' ||
    f.sentView ||
    f.ended?.actions.includes('done')
  ) {
    buttons.push({ id: 'done', label: 'Done', primary: true });
  } else if (info.board) {
    buttons.push({
      id: 'post',
      label: again ? 'Post again' : 'Post…',
      primary: true,
      // The game holds one note, so Post… waits while another board's
      // note is there (Note Editor board 7).
      disabled: !f.canPost || f.ended?.actions.includes('clear-other') === true,
    });
  } else if (f.phase === 'sent') {
    if (info.check) buttons.push({ id: 'check', label: checkLabel, disabled: !f.live });
    buttons.push({ id: 'done', label: 'Done', primary: true });
  } else {
    // The game holds the text as the card shows it, so the check sits
    // beside Done, and Send to game takes Done's place once it differs
    // (Note Editor board 6). After a drop Send again stays.
    if (f.matches && info.check) {
      buttons.push({ id: 'check', label: checkLabel, disabled: !f.live });
      if (!again) {
        buttons.push({ id: 'done', label: 'Done', primary: true });
        return { left, buttons };
      }
    }
    buttons.push({
      id: 'send',
      label: again ? 'Send again' : 'Send to game',
      primary: true,
      disabled: !f.canSend,
    });
  }
  return { left, buttons };
}
