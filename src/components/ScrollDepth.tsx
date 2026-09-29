import { useEffect, useSyncExternalStore } from 'react';
import { listen } from '@tauri-apps/api/event';

interface Props {
  /** True while the find bar is open, so the readout drops below it
   *  instead of sitting under it. */
  findOpen?: boolean;
}

interface Depth {
  offset: number;
  max: number;
}

// Scroll depth readout for the macOS underlay. The native renderer no
// longer draws its own pill there. It reports `vosh://native-scroll`
// as `[offset, max]` whenever the depth changes, and this chip shows
// it at the terminal's top right while you are scrolled back. The
// event only fires on a change, so the last value lives at module
// scope and a remount reads it at once instead of waiting for the
// next scroll. Off the underlay nothing emits and the chip never
// shows. Mount it inside the positioned terminal area.
let depth: Depth = { offset: 0, max: 0 };
const listeners = new Set<() => void>();
let started = false;

function setDepth(next: Depth) {
  if (next.offset === depth.offset && next.max === depth.max) return;
  depth = next;
  for (const cb of listeners) cb();
}

function startScrollDepth() {
  if (started) return;
  started = true;
  listen<unknown>('vosh://native-scroll', (event) => {
    const p = event.payload;
    if (!Array.isArray(p)) return;
    const offset = Number(p[0]);
    const max = Number(p[1]);
    if (!Number.isFinite(offset) || !Number.isFinite(max)) return;
    setDepth({ offset: Math.max(0, offset), max: Math.max(0, max) });
  }).catch(() => {
    // Outside Tauri there is no event bus. Allow a later mount to retry.
    started = false;
  });
}

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

const getDepth = () => depth;

export function ScrollDepth({ findOpen = false }: Props = {}) {
  useEffect(startScrollDepth, []);
  const { offset, max } = useSyncExternalStore(subscribe, getDepth);

  if (offset <= 0) return null;

  return (
    <div className={`ov-depth${findOpen ? ' is-below-find' : ''}`}>
      <svg
        width="12"
        height="12"
        viewBox="0 0 16 16"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.25"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <path d="M4.5 9.75L8 6.25l3.5 3.5" vectorEffect="non-scaling-stroke" />
      </svg>
      <span>{offset.toLocaleString()}</span>
      <span className="ov-depth-of">/ {max.toLocaleString()}</span>
    </div>
  );
}
