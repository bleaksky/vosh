import { createStore } from './store';

export type ToastKind = 'success' | 'info' | 'error';

/** The one button a toast can carry, such as Undo. Pressing it runs
 *  `run` and closes the toast. */
export interface ToastAction {
  label: string;
  run: () => void;
}

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  /** Optional right-aligned mono detail (host:port, reason, file). */
  meta?: string;
  /** The meta reads in the terminal's face, as prompt codes do. */
  metaMono?: boolean;
  action?: ToastAction;
}

export interface ToastInput {
  kind: ToastKind;
  message: string;
  meta?: string;
  /** The meta reads in the terminal's face, as prompt codes do. */
  metaMono?: boolean;
  action?: ToastAction;
  /** Auto-dismiss delay override. Defaults below apply otherwise. */
  timeoutMs?: number;
  /** Stays up until you click it, for a notice that holds until you act. */
  sticky?: boolean;
}

const DEFAULT_TIMEOUT_MS = 5000;
const ERROR_TIMEOUT_MS = 8000;

// Module-level toast queue. Producers call pushToast from anywhere
// (session state handlers, command results), and the Toasts component
// subscribes and renders whatever is queued. Every toast but a sticky
// one dismisses itself on a timer the store owns, an error after a
// longer wait, and dismissToast closes one by hand at any time.
const store = createStore<Toast[]>([]);
let nextId = 1;
const timers = new Map<number, number>();

export function pushToast(input: ToastInput): number {
  const id = nextId++;
  const toast: Toast = { id, kind: input.kind, message: input.message };
  if (input.meta !== undefined) toast.meta = input.meta;
  if (input.metaMono) toast.metaMono = true;
  if (input.action) toast.action = input.action;
  if (!input.sticky) {
    const delay =
      input.timeoutMs ?? (input.kind === 'error' ? ERROR_TIMEOUT_MS : DEFAULT_TIMEOUT_MS);
    timers.set(
      id,
      window.setTimeout(() => dismissToast(id), delay),
    );
  }
  store.set([...store.get(), toast]);
  return id;
}

export function dismissToast(id: number): void {
  const timer = timers.get(id);
  if (timer !== undefined) {
    window.clearTimeout(timer);
    timers.delete(id);
  }
  const toasts = store.get();
  if (!toasts.some((t) => t.id === id)) return;
  store.set(toasts.filter((t) => t.id !== id));
}

export const getToasts = store.get;

export function subscribeToasts(cb: (toasts: Toast[]) => void): () => void {
  return store.subscribe(() => cb(store.get()));
}
