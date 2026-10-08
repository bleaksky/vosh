import type { JobProgress, JobResult, WritingKind } from '../ipc/writing';
import { codeSlot } from './gameCodes';
import { KINDS } from './kinds';
import {
  columns,
  count,
  EDITOR_ROOM,
  innerCodes,
  leadingCode,
  marksCharacter,
  startsAsCommand,
  type Count,
  type Folded,
} from './text';

// What the writing card says: the header's line about the draft, the
// footer's count, the note about the line the caret is on, and what a
// job's end leaves. Each is short and plain, in the boards' words.

/** A footer note: its first sentence reads stronger, and its tone sets
 *  its dot. */
export interface Note {
  lead: string;
  rest: string;
  tone: 'bad' | 'warn' | 'info' | 'ok';
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** One, two... as the boards spell a small count in prose. */
function spelled(n: number): string {
  return (
    ['no', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine'][n] ?? String(n)
  );
}

/** The room the game gives a bug or typo report, less the line who
 *  reported it, the room and the level take at post (recycle.c:2401). */
export function roomFor(kind: WritingKind): number {
  return kind === 'bug' || kind === 'typo' ? 4550 : EDITOR_ROOM;
}

/** The footer's count. A description counts its lines against the help's
 *  ten to thirty, and every other kind against the room the game's
 *  editor gives. Empty lines are named apart (Description Editor Q8). */
export function countLine(
  kind: WritingKind,
  c: Count,
): { main: string; tone: 'n' | 'warn' | 'bad'; rest: string } {
  if (c.lines === 0) return { main: 'Empty', tone: 'n', rest: '' };
  const empty = c.empty > 0 ? `, ${c.empty} empty` : '';
  if (kind === 'description') {
    const main = plural(c.lines, 'line');
    let rest = `${empty} · 10 to 30`;
    let tone: 'n' | 'warn' | 'bad' = 'n';
    if (c.lines < 10) {
      tone = 'warn';
      rest = `${empty}, ${10 - c.lines} short of 10`;
    } else if (c.lines > 30) {
      tone = 'bad';
      rest = `${empty}, ${c.lines - 30 === 1 ? 'one' : c.lines - 30} past 30`;
    }
    if (c.past > 0) rest += ` · ${c.past} past 75`;
    return { main, tone, rest };
  }
  const room = roomFor(kind);
  const tone = c.bytes > room ? 'bad' : 'n';
  return {
    main: plural(c.lines, 'line'),
    tone,
    rest: `${empty} · ${c.bytes.toLocaleString('en-US')} of ${room.toLocaleString('en-US')} characters`,
  };
}

/** What is wrong with the line the caret is on, and its fix, or null
 *  for a clean line. */
export function lineNote(
  line: string,
  row: number,
  width: number,
  helpWidth: boolean,
  immortal: boolean,
): { note: Note; rewrap: boolean } | null {
  const n = row + 1;
  const cols = columns(line);
  if (cols > width) {
    const words = line.replace(/ +$/, '').trimStart();
    if (!words.includes(' ')) {
      return {
        note: {
          lead: `Line ${n} has no space to break at.`,
          rest: ' Add one where you want the line to end.',
          tone: 'bad',
        },
        rewrap: false,
      };
    }
    const past = cols - width;
    return {
      note: {
        lead: `Line ${n} is ${spelled(past)} ${past === 1 ? 'character' : 'characters'} too long.`,
        rest: ` Lines stop at ${width}.`,
        tone: helpWidth ? 'bad' : 'warn',
      },
      rewrap: true,
    };
  }
  if (startsAsCommand(line)) {
    const what = { '.': 'a dot', '@': 'an @', '!': 'an !' }[line[0]] ?? 'a dot';
    return {
      note: {
        lead: `Line ${n} starts with ${what}.`,
        rest: ' The game would take that as a command, so Vosh sends it in a way that keeps it as text.',
        tone: 'warn',
      },
      rewrap: false,
    };
  }
  if (!immortal && innerCodes(line).length > 0) {
    const code = leadingCode(line);
    const kept = code ? codeSlot(code)?.name : null;
    return {
      note: {
        lead: `Line ${n} changes color partway through.`,
        rest: kept
          ? ` The game only keeps a color at the start of a line, so the rest stays ${kept}.`
          : ' The game only keeps a color at the start of a line, so this one won’t show.',
        tone: 'warn',
      },
      rewrap: false,
    };
  }
  const quotes = [...line].filter((c) => marksCharacter(c) === 'quote').length;
  if (quotes > 0) {
    return {
      note: {
        lead: `Line ${n} has ${quotes === 1 ? 'a double quote' : 'double quotes'}.`,
        rest: ` The game turns ${quotes === 1 ? 'it into a single one' : 'them into single ones'}.`,
        tone: 'warn',
      },
      rewrap: false,
    };
  }
  if ([...line].some((c) => marksCharacter(c) === 'dropped')) {
    return {
      note: {
        lead: `Line ${n} has a character the game leaves out.`,
        rest: ' Take it out or swap in a plain one.',
        tone: 'warn',
      },
      rewrap: false,
    };
  }
  return null;
}

/** What a paste changed, for the footer. */
export function pasteNote(
  wrapped: number,
  width: number,
  folded: Folded[],
  word: string | null,
): Note {
  const parts: string[] = [];
  if (wrapped > 0) parts.push(`wrapped ${plural(wrapped, 'line')} of your paste at ${width}`);
  if (folded.length > 0) {
    const what = folded.map((f) => `${f.count} ${f.what}`).join(', ');
    parts.push(`straightened ${what}${word ? ` in ${word}` : ''}`);
  }
  const said = parts.join(' and ');
  return { lead: '', rest: `Vosh ${said}.`, tone: 'info' };
}

/** The header's line about the draft and who it is for. */
export function metaLine(input: {
  name: string;
  board: boolean;
  job: JobProgress | null;
  read: boolean;
  fresh: boolean;
  done: 'sent' | 'posted' | null;
  dropped: { sent: number; total: number } | null;
}): string {
  const { name, board, job } = input;
  if (job) {
    if (job.action === 'post') return `Posting for ${name}`;
    if (job.action === 'find') return `Checking the board for ${name}`;
    if (job.action === 'send' || job.action === 'paste') return `Sending for ${name}`;
    return `Reading for ${name}`;
  }
  if (input.dropped)
    return `${input.dropped.sent} of ${input.dropped.total} lines sent for ${name}`;
  if (input.done === 'posted') return `Posted for ${name}`;
  if (input.done === 'sent') return `Sent for ${name}`;
  if (input.read) return `From the game, for ${name}`;
  if (input.fresh) return `New draft for ${name}`;
  return `${name}’s draft, not ${board ? 'posted' : 'sent'} yet`;
}

/** What the footer says while a job runs. */
export function progressLine(job: JobProgress): string {
  switch (job.stage) {
    case 'sending':
      return `Sending line ${Math.min(job.sent + 1, job.total)} of ${job.total}`;
    case 'fields':
      return 'Setting To and Subject';
    case 'opening':
      return 'Opening the game’s editor';
    case 'checking':
      return 'Checking what the game got';
    case 'closing':
      return 'Closing the game’s editor';
    case 'posting':
      return 'Posting';
    case 'reading':
      return 'Reading from the game';
    default:
      return 'Waiting for the game';
  }
}

/** Who reads a note once it posts, for the confirm. */
export function readers(kind: WritingKind, to: string): string {
  if (KINDS[kind].toImmortal || kind === 'application') {
    return 'Only you and the immortals can read it';
  }
  const words = to.toLowerCase().split(/\s+/);
  if (words.includes('all')) return 'Everyone can read it';
  return `Everyone it’s to can read it`;
}

/** What a job's end says in the footer, for the ends that need words.
 *  The game's own answer stays in the terminal under the card. */
export function resultNote(result: JobResult, kind: WritingKind): Note | null {
  const board = KINDS[kind].board;
  switch (result.kind) {
    case 'refused': {
      const where = {
        to: 'The game wouldn’t take that To.',
        subject: 'The game wouldn’t take that subject.',
        language: 'The game wouldn’t take that language.',
        editor: 'The game didn’t open its editor.',
        post:
          kind === 'application'
            ? 'The game turned your application down.'
            : 'The game didn’t post it.',
      }[result.field];
      return {
        lead: where,
        rest: ' It says why just below. Your draft is safe.',
        tone: 'bad',
      };
    }
    case 'busy':
      return board
        ? {
            lead: 'You’re still in another editor in the game.',
            rest: ' Type @ to close it, then post.',
            tone: 'warn',
          }
        : {
            lead: 'The game’s editor is still open.',
            rest: ' Type @ to close it, then send.',
            tone: 'warn',
          };
    case 'too_long':
      return {
        lead: 'The game ran out of room',
        rest: ` after line ${result.sent}. Make it shorter and send it again.`,
        tone: 'bad',
      };
    case 'failed':
      return failedNote(result.why, result.line);
    case 'stopped':
      return {
        lead: `You stopped after ${plural(result.sent, 'line')}.`,
        rest: board ? ' Nothing was posted.' : ' The game kept the lines it got.',
        tone: 'warn',
      };
    case 'dropped':
      if (board) {
        return result.posted
          ? {
              lead: 'Your connection dropped while this was posting.',
              rest: ' Check the board before you post it again.',
              tone: 'bad',
            }
          : {
              lead: `Your connection dropped after line ${result.sent}.`,
              rest: ' Nothing was posted.',
              tone: 'bad',
            };
      }
      return {
        lead: `Your connection dropped after line ${result.sent}.`,
        rest: ` The game kept ${result.sent === 1 ? 'that line' : `those ${result.sent} lines`}.`,
        tone: 'bad',
      };
    case 'found':
      return {
        lead: '',
        rest: `It went through before your connection dropped. It’s note ${result.number} on the board.`,
        tone: 'ok',
      };
    case 'cant_tell':
      return {
        lead: 'Your connection dropped while this was posting.',
        rest: ` ${pluralTitle(kind)} don’t show up anywhere you can check, so there’s no telling if it went through.`,
        tone: 'warn',
      };
    case 'other_note': {
      const what = result.board ? KINDS[result.board].title.toLowerCase() : 'note';
      return {
        lead: `You’d already started ${/^[aeiou]/.test(what) ? 'an' : 'a'} ${what} in the game.`,
        rest: result.note ? ' It’s in your drafts now.' : ' Vosh couldn’t get its text.',
        tone: 'warn',
      };
    }
    case 'offer_gone':
      return {
        lead: 'Too late to open that one here.',
        rest: ' Open the card again from the Write menu.',
        tone: 'warn',
      };
    default:
      return null;
  }
}

function failedNote(why: string, line: number | null): Note {
  switch (why) {
    case 'mend':
      return {
        lead: `Line ${line ?? 1} still isn’t right in the game.`,
        rest: ' Vosh closed the editor. Send again, or put back what the game had.',
        tone: 'bad',
      };
    case 'no_mark':
      return {
        lead: `Vosh couldn’t fix line ${line ?? 1}.`,
        rest: ' It has every quote and bracket Vosh could use. Take one out and send again.',
        tone: 'bad',
      };
    case 'closed':
      return {
        lead: 'The game’s editor closed before Vosh finished.',
        rest: ' Send again to finish.',
        tone: 'bad',
      };
    case 'silent':
      return {
        lead: 'The game stopped answering.',
        rest: ' Your draft is safe.',
        tone: 'bad',
      };
    case 'differs':
      return {
        lead:
          line === null
            ? 'Your note came out different in the game,'
            : `Line ${line} came out different in the game,`,
        rest: ' so Vosh cleared the note and didn’t post it.',
        tone: 'bad',
      };
    case 'bad_dot':
      return {
        lead: 'The game’s editor didn’t understand a command.',
        rest: ' Vosh closed it. Your draft is safe.',
        tone: 'bad',
      };
    default:
      return {
        lead: 'Vosh couldn’t get your text from the game.',
        rest: ' Try Read again.',
        tone: 'bad',
      };
  }
}

/** A board's kind in the plural, for a sentence's start. */
function pluralTitle(kind: WritingKind): string {
  const title = KINDS[kind].title;
  return title.endsWith('y') ? `${title.slice(0, -1)}ies` : `${title}s`;
}

/** How many lines the game holds of `game`, for the header. */
export function lineCount(lines: readonly string[]): number {
  return count(lines, 75).lines;
}

/** What the footer says under the preview. */
export function previewLine(kind: WritingKind): string {
  if (!KINDS[kind].board) return 'This is how a looker sees it';
  if (KINDS[kind].toImmortal || kind === 'application')
    return 'This is how the immortals will see it';
  return 'This is how readers will see it';
}
