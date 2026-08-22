// Well-splits state: whether the terminal well is split into the
// three-pane layout (session / chat / raw log) from the Ember canvas.
// A module store rather than App state so the palette, the context
// menu, and the status bar can all read and toggle it without prop
// drilling. Persisted so the layout survives relaunch.

const KEY = 'vosh.well.splits';

let open = false;
try {
  open = localStorage.getItem(KEY) === '1';
} catch {
  // storage unavailable; session-only default
}

const subs = new Set<(open: boolean) => void>();

export function wellSplitsOpen(): boolean {
  return open;
}

export function setWellSplits(next: boolean): void {
  if (next === open) return;
  open = next;
  try {
    localStorage.setItem(KEY, next ? '1' : '0');
  } catch {
    // best-effort persistence
  }
  for (const cb of subs) cb(open);
}

export function toggleWellSplits(): void {
  setWellSplits(!open);
}

export function subscribeWellSplits(cb: (open: boolean) => void): () => void {
  subs.add(cb);
  return () => {
    subs.delete(cb);
  };
}
