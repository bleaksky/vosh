import { useEffect, useSyncExternalStore } from 'react';
import { getNativeScroll, startNativeScroll, subscribeNativeScroll } from '../lib/nativeScroll';
import { nativeUnderlay } from './Terminal';

interface Props {
  /** True while the find bar is open, so the readout drops below it
   *  instead of sitting under it. */
  findOpen?: boolean;
}

// Scroll depth readout for the macOS underlay. The native renderer no
// longer draws its own pill there, so this chip shows the depth that
// lib/nativeScroll tracks at the terminal's top right while you are
// scrolled back. The on top surface on Windows and Linux reports its
// depth too but still draws its own pill, so the chip stays off there.
// Mount it inside the positioned terminal area.

export function ScrollDepth({ findOpen = false }: Props = {}) {
  useEffect(startNativeScroll, []);
  const { offset, max } = useSyncExternalStore(subscribeNativeScroll, getNativeScroll);

  if (offset <= 0 || !nativeUnderlay()) return null;

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
