import type { WritingKind } from '../ipc/writing';
import { KINDS } from './kinds';
import type { Count } from './text';
import { readers, type Note } from './words';

// What the writing card asks before it does what can't be taken back,
// and what it says once the game took a text (Description Editor Q13,
// Note Editor Q7 and Q8). Each confirm says who reads the text or what
// goes, in short plain words.

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
        ? ` You began it in ${began}, and the game records ${here}, where you are now.`
        : ` The game records ${here}, where you are now.`
      : '';
  return {
    title: `Post your ${named(kind)}?`,
    body: `${readers(kind, to)}, and you can’t change it once it’s posted.${where}`,
    label: 'Post',
    tone: 'primary',
  };
}

/** Send for approval…'s confirm lists what the note carries, and first
 *  names a count outside ten to thirty or a line past 75. History's is
 *  shorter, and it goes once. */
export function checkAsk(kind: WritingKind, counted: Count): Ask {
  if (kind !== 'description') {
    return {
      title: 'Send your history for review?',
      body: 'An immortal will read it as the game has it now. You can only send it once.',
      label: 'Send for review',
      tone: 'primary',
    };
  }
  const warn =
    counted.lines < 10 || counted.lines > 30
      ? `Your description has ${counted.lines} lines with text, outside the help’s ten to thirty. `
      : counted.past > 0
        ? `${counted.past === 1 ? 'A line runs' : `${counted.past} lines run`} past 75. `
        : '';
  return {
    title: 'Send your description for approval?',
    body: `${warn}The immortals get your description as the game holds it now, with your race, class, birth, age and hand, any perks, and your face, hair and body if you set them. The game takes one check at a time, and a note tells you when they decide.`,
    label: 'Send dcheck',
    tone: 'primary',
  };
}

/** The game holds another text than your draft began from. */
export function changedAsk(kind: WritingKind): Ask {
  return {
    title: `Your ${named(kind)} changed in the game`,
    body: 'It changed since your draft began. Send yours over it, or keep the game’s copy and read it first.',
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
    body: `The game holds another ${what} of yours, about ${subject || 'nothing yet'}. Vosh keeps it in your drafts and clears it before it posts this one.`,
    label: 'Replace it',
    tone: 'primary',
  };
}

/** Read again replaces the draft. */
export function readAgainAsk(kind: WritingKind): Ask {
  return {
    title: `Read your ${named(kind)} again?`,
    body: 'Vosh replaces your draft with what the game holds.',
    label: 'Read again',
    tone: 'primary',
  };
}

export function clearOtherAsk(kind: WritingKind): Ask {
  return {
    title: `Clear the ${named(kind)} in the game?`,
    body: 'Your copy stays in your drafts, so you can post it later.',
    label: 'Clear it',
  };
}

export const DELETE_ASK: Ask = {
  title: 'Delete the draft?',
  body: 'Until it posts, the draft is the only copy, so it’s gone for good.',
  label: 'Delete',
};

export const CLEAR_ASK: Ask = {
  title: 'Clear the draft?',
  body: 'Your draft goes, and the game keeps what it holds.',
  label: 'Clear',
};

/** The read back after a send. */
export function sentNote(held: number, all: boolean): Note {
  return all
    ? { lead: '', rest: `The game has all ${held} lines as you wrote them`, tone: 'ok' }
    : {
        lead: 'The game holds a different text.',
        rest: ' Read again to see what it holds.',
        tone: 'warn',
      };
}

/** A post the game took, a cabal's vote, or a report the forum missed. */
export function postedNote(kind: WritingKind, forum: boolean, vote: boolean): Note {
  const rest = vote
    ? 'Sent. The cabal votes on it now.'
    : !forum
      ? 'Posted. The forum didn’t get a copy.'
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
          ? 'The game takes one at a time, so the check under review keeps the text you sent then.'
          : 'The game takes your history once, and it already has it.',
      tone: 'warn',
    };
  }
  return {
    lead: '',
    rest:
      kind === 'description'
        ? 'Sent for approval. A note tells you when the immortals decide.'
        : 'Sent for review. An immortal will read it.',
    tone: 'ok',
  };
}
