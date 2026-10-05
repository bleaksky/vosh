import type { XtermPalette } from './themes';

// The game color fit (theme/gameFit) takes about two seconds of CPU, so a
// module worker runs it and the window never waits on it. A window
// starts its worker the first time it fits, and the worker answers in
// the order it was asked.

type Fitted = Partial<XtermPalette>;

let worker: Worker | null = null;
let nextId = 0;
const waiting = new Map<number, (fitted: Fitted | null) => void>();

function startWorker(): Worker | null {
  try {
    const w = new Worker(new URL('./gameFit.worker.ts', import.meta.url), { type: 'module' });
    w.onmessage = (e: MessageEvent<{ id: number; fitted: Fitted | null }>) => {
      waiting.get(e.data.id)?.(e.data.fitted);
      waiting.delete(e.data.id);
    };
    // A worker that fails to load answers nothing, so every fit it
    // holds ends unfitted and the next fit starts a new worker.
    w.onerror = () => {
      w.terminate();
      if (worker === w) worker = null;
      for (const answer of waiting.values()) answer(null);
      waiting.clear();
    };
    return w;
  } catch {
    return null;
  }
}

/** The fit of `palette`, the slots it moves, or null when this window
 *  cannot fit it: a color that is not hex, or a webview that runs no
 *  module worker. */
export function fitOffThread(palette: XtermPalette): Promise<Fitted | null> {
  worker ??= startWorker();
  const w = worker;
  if (!w) return Promise.resolve(null);
  const id = nextId++;
  return new Promise((resolve) => {
    waiting.set(id, resolve);
    w.postMessage({ id, palette });
  });
}
