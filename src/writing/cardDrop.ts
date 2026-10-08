import type { JobResult, WriteJob, WritingKind } from '../ipc/writing';
import type { Ended } from './cardFoot';
import { KINDS } from './kinds';
import { resultNote } from './words';

// What the writing card does after a drop (Note Editor board 8). A drop
// mid send says how far it got. A drop after the post went out waits
// for a look at the board's list, which runs once the session plays
// again, and only then says whether it posted or offers Post again.

/** How far a send got before the link dropped. */
export interface Drop {
  sent: number;
  total: number;
}

/** A look at the board's list that waits, for the subject the post went
 *  out with, so an edit to the field meanwhile changes nothing. */
export interface Find {
  drop: Drop;
  subject: string;
  /** How many notes of yours with the subject the board listed just
   *  before the post, or null when Vosh couldn't read the list. */
  baseline: number | null;
  started: boolean;
}

export interface DropState {
  dropped: Drop | null;
  find: Find | null;
  ended: Ended;
  /** The note is on its board, so it moves to Sent. */
  posted: boolean;
}

/** The card after `job` ended with `result`, for the ends a drop and a
 *  find bring, or null for any other. `total` counts the card's lines. */
export function afterDrop(
  find: Find | null,
  result: JobResult,
  job: WriteJob,
  kind: WritingKind,
  total: number,
): DropState | null {
  const note = (r: JobResult) => resultNote(r, kind)!;
  switch (result.kind) {
    case 'dropped': {
      const drop = find?.drop ?? { sent: result.sent, total };
      // Whether it posted waits for the board's list, so Post again
      // shows once the list says. A find the drop cut runs again.
      if (result.posted || job.action === 'find') {
        return {
          dropped: drop,
          find: {
            drop,
            subject: find?.subject ?? job.subject ?? '',
            baseline: find ? find.baseline : (result.baseline ?? null),
            started: false,
          },
          ended: { note: note({ ...result, posted: true }), actions: [] },
          posted: false,
        };
      }
      return {
        dropped: drop,
        find: null,
        ended: {
          note: note(result),
          actions: KINDS[kind].board ? ['again'] : ['restore', 'again'],
        },
        posted: false,
      };
    }
    case 'found':
      return {
        dropped: null,
        find: null,
        ended: { note: note(result), actions: [] },
        posted: true,
      };
    case 'not_found':
    case 'cant_tell': {
      const drop = find?.drop ?? { sent: total, total };
      const said =
        result.kind === 'cant_tell'
          ? note(result)
          : note({ kind: 'dropped', sent: drop.sent, posted: false });
      return {
        dropped: drop,
        find: null,
        ended: { note: said, actions: ['again'] },
        posted: false,
      };
    }
    case 'stopped':
      if (job.action !== 'find') return null;
      return {
        dropped: find?.drop ?? null,
        find: null,
        ended: { note: note({ kind: 'dropped', sent: 0, posted: true }), actions: ['again'] },
        posted: false,
      };
    default:
      return null;
  }
}

/** The find to start now: one that waits while the session plays and
 *  no job runs. */
export function findToStart(find: Find | null, live: boolean, jobRuns: boolean): Find | null {
  return find && !find.started && live && !jobRuns ? find : null;
}
