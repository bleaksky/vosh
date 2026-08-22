export type ToastKind = 'success' | 'info' | 'error';

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  /** Optional right-aligned mono detail (host:port, reason, file). */
  meta?: string;
}

export interface ToastInput {
  kind: ToastKind;
  message: string;
  meta?: string;
  /** Auto-dismiss delay override. Defaults below apply otherwise. */
  timeoutMs?: number;
}

const DEFAULT_TIMEOUT_MS = 5000;
const ERROR_TIMEOUT_MS = 8000;

// Module-level toast queue, same shape as chatStore: producers call
// pushToast from anywhere (session state handlers, command results),
// the Toasts component subscribes and renders whatever is queued.
// Every toast self-dismisses on a store-owned timer — errors linger
// longer — and dismissToast is always available for a manual close.
let toasts: Toast[] = [];
let listeners: Array<(toasts: Toast[]) => void> = [];
let nextId = 1;
const timers = new Map<number, number>();

function notify() {
  const snapshot = toasts;
  for (const l of listeners) l(snapshot);
}

export function pushToast(input: ToastInput): number {
  const id = nextId++;
  const toast: Toast = { id, kind: input.kind, message: input.message };
  if (input.meta !== undefined) toast.meta = input.meta;
  toasts = [...toasts, toast];
  const delay = input.timeoutMs ?? (input.kind === 'error' ? ERROR_TIMEOUT_MS : DEFAULT_TIMEOUT_MS);
  timers.set(
    id,
    window.setTimeout(() => dismissToast(id), delay),
  );
  notify();
  return id;
}

export function dismissToast(id: number): void {
  const timer = timers.get(id);
  if (timer !== undefined) {
    window.clearTimeout(timer);
    timers.delete(id);
  }
  if (!toasts.some((t) => t.id === id)) return;
  toasts = toasts.filter((t) => t.id !== id);
  notify();
}

export function getToasts(): Toast[] {
  return toasts;
}

export function subscribeToasts(cb: (toasts: Toast[]) => void): () => void {
  listeners.push(cb);
  return () => {
    listeners = listeners.filter((l) => l !== cb);
  };
}
