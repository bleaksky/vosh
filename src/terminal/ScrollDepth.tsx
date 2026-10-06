import { useEffect, useSyncExternalStore } from 'react';
import { getNativeScroll, startNativeScroll, subscribeNativeScroll } from './native/nativeScroll';
import { nativeSurfaceEnabled } from './terminalRenderer';

interface Props {
  /** True while the find bar is open, so the readout drops below it
   *  instead of sitting under it. */
  findOpen?: boolean;
}

// Scroll depth readout for the native surface. This chip shows the
// depth that terminal/native/nativeScroll tracks at the terminal's top right while
// you are scrolled back. Mount it inside the positioned terminal area.

export function ScrollDepth({ findOpen = false }: Props = {}) {
  useEffect(startNativeScroll, []);
  const { offset, max } = useSyncExternalStore(subscribeNativeScroll, getNativeScroll);

  if (offset <= 0 || !nativeSurfaceEnabled()) return null;

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
