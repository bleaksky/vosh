import { useEffect, useSyncExternalStore } from 'react';
import { getNativeScroll, startNativeScroll, subscribeNativeScroll } from './native/nativeScroll';
import { nativeSurfaceEnabled } from './terminalRenderer';

interface Props {
  /** True while the find bar is open, so the readout drops below it
   *  instead of sitting under it. */
  findOpen?: boolean;
  /** How far back xterm's history pane shows, while the split is open. */
  history?: { back: number; max: number } | null;
}

// Scroll depth readout, one chip for both renderers. It sits at the
// terminal's top right while you are scrolled back. On the native
// surface it reads the depth terminal/native/nativeScroll tracks, and on
// xterm the depth of the history pane. Mount it inside the positioned
// terminal area.

export function ScrollDepth({ findOpen = false, history = null }: Props = {}) {
  useEffect(startNativeScroll, []);
  const native = useSyncExternalStore(subscribeNativeScroll, getNativeScroll, getNativeScroll);
  const { offset, max } = nativeSurfaceEnabled()
    ? native
    : { offset: history?.back ?? 0, max: history?.max ?? 0 };

  if (offset <= 0 || max <= 0) return null;

  return (
    <div className={`ov-depth${findOpen ? ' is-below-find' : ''}`} aria-live="polite">
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
