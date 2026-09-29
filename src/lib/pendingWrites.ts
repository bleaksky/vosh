import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// Writes a window holds back for a moment, and the one place that sends
// them all at once. Settings saves a change after a short pause, the
// pane layout waits out a splitter drag, and a number or color field
// saves when you leave it. Closing the Settings window, or quitting
// Vosh, sends every one of them first, so the last change you made
// reaches the disk.

/** Sends what one writer holds now. */
export type Flush = () => Promise<void> | void;

export interface PendingWrites {
  /** Add a writer's flush. Returns the function that takes it out. */
  register: (flush: Flush) => () => void;
  /** Run every flush at once and wait for them, up to `timeoutMs`. A
   *  flush that fails does not stop the others. Resolves true when
   *  every flush finished in time and none failed. */
  flushAll: (timeoutMs?: number) => Promise<boolean>;
}

/** How long a window waits on its own writes before it gives up. On
 *  quit the backend waits a little longer on the window, see
 *  exit_flush.rs. */
export const FLUSH_TIMEOUT_MS = 800;

export function createPendingWrites(): PendingWrites {
  const flushes = new Set<Flush>();
  return {
    register(flush) {
      flushes.add(flush);
      return () => {
        flushes.delete(flush);
      };
    },
    async flushAll(timeoutMs = FLUSH_TIMEOUT_MS) {
      const runs = [...flushes].map(async (flush) => {
        await flush();
      });
      const settled = Promise.allSettled(runs).then((results) => {
        for (const r of results) {
          if (r.status === 'rejected') console.error('[writes] a pending write failed', r.reason);
        }
        return results.every((r) => r.status === 'fulfilled');
      });
      let timer: ReturnType<typeof setTimeout> | undefined;
      const late = new Promise<boolean>((resolve) => {
        timer = setTimeout(() => resolve(false), timeoutMs);
      });
      try {
        return await Promise.race([settled, late]);
      } finally {
        clearTimeout(timer);
      }
    },
  };
}

/** This window's writers. */
export const pendingWrites = createPendingWrites();

/** One value sent after a pause, the latest one winning, like the
 *  Settings autosave. Flush sends the waiting value at once. */
export interface DebouncedWrite<T> {
  /** Send `value` after `delayMs`, in place of any value waiting. */
  schedule: (value: T, delayMs: number) => void;
  /** Change the value waiting, if there is one. */
  patch: (fn: (value: T) => T) => void;
  hasPending: () => boolean;
  /** Send the waiting value now. Resolves once `send` has. */
  flush: () => Promise<void>;
  /** Forget the waiting value. */
  drop: () => void;
}

/** A DebouncedWrite over `send`, which should handle its own errors,
 *  since a timer runs it too. */
export function createDebouncedWrite<T>(send: (value: T) => Promise<void>): DebouncedWrite<T> {
  let waiting: { value: T } | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const stopTimer = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  };
  const flush = async () => {
    stopTimer();
    const next = waiting;
    waiting = null;
    if (next) await send(next.value);
  };
  return {
    schedule(value, delayMs) {
      stopTimer();
      waiting = { value };
      timer = setTimeout(() => void flush(), delayMs);
    },
    patch(fn) {
      if (waiting) waiting = { value: fn(waiting.value) };
    },
    hasPending: () => waiting !== null,
    flush,
    drop() {
      stopTimer();
      waiting = null;
    },
  };
}

/** What commitFocusedField needs of the focused element. */
interface Focusable {
  tagName?: string;
  blur?: () => void;
}

/** Leave the focused field, so a field that saves when you leave it,
 *  like a number or a color, saves what it holds. Then wait a tick, so
 *  React passes the change on to the writer that sends it. */
export async function commitFocusedField(
  doc: { activeElement: unknown } | undefined = typeof document === 'undefined'
    ? undefined
    : document,
): Promise<void> {
  const el = doc?.activeElement as Focusable | null | undefined;
  const tag = el?.tagName?.toUpperCase();
  if (!el || typeof el.blur !== 'function') return;
  if (tag !== 'INPUT' && tag !== 'TEXTAREA' && tag !== 'SELECT') return;
  el.blur();
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
}

/** Send everything this window holds: the focused field first when
 *  `commitFocus` is on, then every registered writer. Never rejects. */
export async function sendPendingWrites(
  options: {
    commitFocus?: boolean;
    writes?: PendingWrites;
    timeoutMs?: number;
    doc?: { activeElement: unknown };
  } = {},
): Promise<boolean> {
  const { commitFocus = false, writes = pendingWrites, timeoutMs, doc } = options;
  try {
    if (commitFocus) await commitFocusedField(doc);
    return await writes.flushAll(timeoutMs);
  } catch (e) {
    console.error('[writes] sending pending writes failed', e);
    return false;
  }
}

// ── Closing the Settings window ─────────────────────────────────────

/** Asks before the window closes over unsaved changes. `proceed`
 *  closes the window after all. */
export type CloseGuard = (proceed: () => void) => void;

let closeGuard: CloseGuard | null = null;

/** Hold a close for a page with unsaved changes. Returns the function
 *  that lets go. */
export function setCloseGuard(guard: CloseGuard): () => void {
  closeGuard = guard;
  return () => {
    if (closeGuard === guard) closeGuard = null;
  };
}

export function currentCloseGuard(): CloseGuard | null {
  return closeGuard;
}

/** What closing the window does: send the pending writes, then close,
 *  or let the guard ask first. The writes go even when the close waits
 *  on the guard, since each one saves on its own. The guard is read
 *  after the writes, since leaving the focused field can leave a page
 *  with unsaved changes. */
export async function runCloseRequest(steps: {
  send: () => Promise<unknown>;
  guard: () => CloseGuard | null;
  close: () => Promise<void> | void;
}): Promise<void> {
  try {
    await steps.send();
  } catch (e) {
    console.error('[writes] sending pending writes failed', e);
  }
  const guard = steps.guard();
  if (guard) {
    guard(() => void steps.close());
    return;
  }
  await steps.close();
}

// ── Quit ────────────────────────────────────────────────────────────

/** The event the backend sends each window when you quit, with a round
 *  number, and the command a window answers with once it has sent what
 *  it held. */
export const FLUSH_REQUEST_EVENT = 'vosh://flush-pending-writes';
const FLUSH_DONE_COMMAND = 'pending_writes_flushed';

/** Send what this window holds when the backend asks on quit, then tell
 *  the backend, which waits a short time for every window before it
 *  writes the profile and exits. The backend asks each window once a
 *  round, so every request gets an answer. */
export function listenForQuitFlush(options: { commitFocus?: boolean } = {}): Promise<UnlistenFn> {
  return listen<unknown>(FLUSH_REQUEST_EVENT, () => {
    void answerQuitFlush(options.commitFocus === true);
  });
}

async function answerQuitFlush(commitFocus: boolean): Promise<void> {
  await sendPendingWrites({ commitFocus });
  await invoke(FLUSH_DONE_COMMAND).catch((e: unknown) =>
    console.error('[writes] telling the backend failed', e),
  );
}
