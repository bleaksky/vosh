import type { WritingKind } from '../ipc/writing';
import { KINDS } from './kinds';
import type { Count } from './text';
import { readers, type Note } from './words';

// What the writing card asks before it does what can't be taken back,
// and what it says once the game took a text. Each confirm says who
// reads the text or what goes, in short plain words.

export interface Ask {
  title: string;
  body: string;
  label: string;
  tone?: 'primary' | 'danger';
  cancel?: string;
}

/** The text's name in a sentence: your description, your note. */
function named(kind: WritingKind): string {
  const title = KINDS[kind].title;
  return title.startsWith('Your ') ? title.slice(5).toLowerCase() : title.toLowerCase();
}

/** Post…'s confirm: who reads it, and for a bug or typo report the room
 *  the game records, which is where you stand as it posts. */
export function postAsk(
  kind: WritingKind,
  to: string,
  began: string | null,
  here: string | null,
): Ask {
  const where =
    KINDS[kind].room && here
      ? began && began !== here
        ? ` You started it in ${began}, but the game will record ${here}, where you are now.`
        : ` The game will record ${here}, where you are now.`
      : '';
  return {
    title: `Post your ${named(kind)}?`,
    body: `${readers(kind, to)}, and you can’t change it once it’s posted.${where}`,
    label: 'Post',
    tone: 'primary',
  };
}

/** Post… asks even with Ask before you post off when a bug or typo
 *  report would record a room other than the one you began it in. */
export function postStillAsks(
  kind: WritingKind,
  began: string | null,
  here: string | null,
): boolean {
  return Boolean(KINDS[kind].room && here && began && began !== here);
}

/** Send for approval…'s confirm lists what the note carries, and first
 *  names a count outside ten to thirty or a line past 75. History's is
 *  shorter, and it goes once. */
export function checkAsk(kind: WritingKind, counted: Count): Ask {
  if (kind !== 'description') {
    return {
      title: 'Send your history for review?',
      body: 'An immortal reads it as the game has it now. You only get to send it once.',
      label: 'Send for review',
      tone: 'primary',
    };
  }
  const warn =
    counted.lines < 10 || counted.lines > 30
      ? `Your description has ${counted.lines} lines, and the help asks for ten to thirty. `
      : counted.past > 0
        ? `${counted.past === 1 ? 'A line runs' : `${counted.past} lines run`} past 75. `
        : '';
  return {
    title: 'Send your description for approval?',
    body: `${warn}The immortals see your description as the game has it now, along with your race, class, birth, age and hand, any perks, and your face, hair and body if you set them. You can only have one check waiting, and you’ll get a note when they decide.`,
    label: 'Send dcheck',
    tone: 'primary',
  };
}

/** The game holds another text than your draft began from. */
export function changedAsk(kind: WritingKind): Ask {
  return {
    title: `Your ${named(kind)} changed in the game`,
    body: 'It’s not what it was when you started this draft. Send yours over it, or read what the game has first.',
    label: 'Send mine',
    tone: 'primary',
    cancel: 'Not now',
  };
}

/** The board holds another note of yours. */
export function sameNoteAsk(kind: WritingKind, subject: string): Ask {
  const what = named(kind);
  return {
    title: `Replace the ${what} in the game?`,
    body: `You already have ${/^[aeiou]/.test(what) ? 'an' : 'a'} ${what} started in the game, ${subject ? `about ${subject}` : 'with no subject yet'}. Vosh saves it to your drafts, then clears it and posts this one.`,
    label: 'Replace it',
    tone: 'primary',
  };
}

/** Read again replaces the draft. */
export function readAgainAsk(kind: WritingKind): Ask {
  return {
    title: `Read your ${named(kind)} again?`,
    body: 'Your draft gets replaced with what the game has.',
    label: 'Read again',
    tone: 'primary',
  };
}

export function clearOtherAsk(kind: WritingKind): Ask {
  return {
    title: `Clear the ${named(kind)} in the game?`,
    body: 'It stays in your drafts, so you can post it later.',
    label: 'Clear it',
  };
}

export const DELETE_ASK: Ask = {
  title: 'Delete the draft?',
  body: 'It hasn’t been posted, so this is the only copy. Once it’s gone, it’s gone.',
  label: 'Delete',
};

export const CLEAR_ASK: Ask = {
  title: 'Clear the draft?',
  body: 'Your draft goes away. What’s in the game stays as it is.',
  label: 'Clear',
};

/** The read back after a send. */
export function sentNote(held: number, all: boolean): Note {
  return all
    ? { lead: '', rest: `The game has all ${held} lines, just as you wrote them`, tone: 'ok' }
    : {
        lead: 'The game has something different.',
        rest: ' Read again to see what it has.',
        tone: 'warn',
      };
}

/** A post the game took, a cabal's vote, or a report the forum missed. */
export function postedNote(kind: WritingKind, forum: boolean, vote: boolean): Note {
  const rest = vote
    ? 'Sent. The cabal will vote on it.'
    : !forum
      ? 'Posted, but it didn’t make it to the forum.'
      : KINDS[kind].toImmortal || kind === 'application'
        ? 'Posted to the immortals.'
        : 'Posted.';
  return { lead: '', rest, tone: 'ok' };
}

/** The game's answer to dcheck or history check, in short. */
export function checkedNote(kind: WritingKind, lines: readonly string[]): Note {
  const said = lines.join(' ');
  if (/already been approved/i.test(said)) {
    return { lead: '', rest: 'Your description is already approved.', tone: 'ok' };
  }
  if (/already submitted|already made your intentions|already recognize/i.test(said)) {
    return {
      lead: '',
      rest:
        kind === 'description'
          ? 'You already have a check waiting, and it uses the text you sent back then.'
          : 'You’ve already sent your history, and the game only takes it once.',
      tone: 'warn',
    };
  }
  return {
    lead: '',
    rest:
      kind === 'description'
        ? 'Sent for approval. You’ll get a note when the immortals decide.'
        : 'Sent for review. An immortal will read it.',
    tone: 'ok',
  };
}
