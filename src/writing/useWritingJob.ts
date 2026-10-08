import { useEffect, useRef } from 'react';
import {
  writingStart,
  writingStop,
  writingTake,
  type JobResult,
  type WriteJob,
  type WritingState,
} from '../ipc/writing';
import { errorText } from '../lib/text';

// The card's jobs in a session's writer. A job runs one at a time, and
// its end comes back on session://writing with its number, which hands
// the result to the card. The numbers start at the time the window
// opened, so an end left from an earlier card never reads as a new one.

let nextId = Date.now();

export type JobSpec = Omit<WriteJob, 'id'>;

export interface WritingJobs {
  /** Start `job`. */
  run: (job: JobSpec) => void;
  /** Open the card on the offer `id`, a read the writer runs. */
  take: (id: number, job: JobSpec) => void;
  stop: () => void;
  /** The card's job under way, or null. */
  running: WritingState['job'];
}

export function useWritingJob(
  session: number,
  writing: WritingState,
  onDone: (result: JobResult, job: WriteJob) => void,
): WritingJobs {
  const pending = useRef<WriteJob | null>(null);
  const done = useRef(onDone);
  done.current = onDone;

  useEffect(() => {
    const end = writing.done;
    const job = pending.current;
    if (!end || !job || end.id !== job.id) return;
    pending.current = null;
    done.current(end.result, job);
  }, [writing.done]);

  const fail = (job: WriteJob) => (e: unknown) => {
    if (pending.current?.id !== job.id) return;
    pending.current = null;
    console.error('[writing] the writer did not hear', errorText(e));
    done.current({ kind: 'dropped', sent: 0, posted: false }, job);
  };

  const ours = writing.job && pending.current && writing.job.id === pending.current.id;
  return {
    run: (spec) => {
      nextId += 1;
      const job = { ...spec, id: nextId };
      pending.current = job;
      void writingStart(job, session).catch(fail(job));
    },
    take: (id, spec) => {
      const job = { ...spec, id };
      pending.current = job;
      void writingTake(id, session).catch(fail(job));
    },
    stop: () => {
      void writingStop(session).catch(() => {});
    },
    running: ours ? writing.job : null,
  };
}
