import { listen } from '@tauri-apps/api/event';
import { NATIVE_SCROLL } from '../ipc/events';

// Scroll depth of the native terminal grid. The native renderer
// reports `vosh://native-scroll` as `[offset, max]` whenever the depth
// changes, on every platform, so the page knows when the grid has split
// to show scrollback. The macOS underlay also draws the readout from it.
// The event only fires on a change, so the last value lives at module
// scope and a remount reads it at once instead of waiting for the next
// scroll. Without the native surface nothing emits and the offset
// stays 0.

export interface NativeScrollDepth {
  /** Lines scrolled back from the live tail. 0 at the tail. */
  offset: number;
  max: number;
}

let depth: NativeScrollDepth = { offset: 0, max: 0 };
const listeners = new Set<() => void>();
let started = false;

function setDepth(next: NativeScrollDepth) {
  if (next.offset === depth.offset && next.max === depth.max) return;
  depth = next;
  for (const cb of listeners) cb();
}

export function startNativeScroll(): void {
  if (started) return;
  started = true;
  listen<unknown>(NATIVE_SCROLL, (event) => {
    const p = event.payload;
    if (!Array.isArray(p)) return;
    const offset = Number(p[0]);
    const max = Number(p[1]);
    if (!Number.isFinite(offset) || !Number.isFinite(max)) return;
    setDepth({ offset: Math.max(0, offset), max: Math.max(0, max) });
  }).catch(() => {
    // Outside Tauri there is no event bus. Allow a later start to retry.
    started = false;
  });
}

export function subscribeNativeScroll(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

export function getNativeScroll(): NativeScrollDepth {
  return depth;
}
