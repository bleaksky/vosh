import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import {
  clearCoach,
  FIND_FRAMES,
  ringBox,
  sameBox,
  tipPlace,
  useCoach,
  type Box,
  type Coach,
} from './coach';

// Show me's coach mark. A 2 px accent ring 2 px
// out from what to pick, with one pulse that reduced motion drops, and
// one line beside it on the toast recipe with no buttons, kept inside
// the window. It reads its targets' rects each frame and draws over
// them, so it follows a menu that moves or a page that scrolls. Focus
// moves to the first target. The pick, Esc or a press anywhere clears
// it, and so does a target that leaves the page, as a menu does when
// it closes. Each window mounts one CoachRing, and showCoach in that
// window draws it.

export function CoachRing() {
  const current = useCoach();
  return current ? <Ring coach={current} /> : null;
}

function Ring({ coach: shown }: { coach: Coach }) {
  const [box, setBox] = useState<Box | null>(null);
  const [tip, setTip] = useState<{ left: number; top: number } | null>(null);
  const tipRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let frame = 0;
    let tries = 0;
    let targets: readonly HTMLElement[] = [];
    const tick = () => {
      if (targets.length === 0) {
        targets = shown.find();
        if (targets.length === 0) {
          tries += 1;
          if (tries >= FIND_FRAMES) clearCoach();
          else frame = requestAnimationFrame(tick);
          return;
        }
        targets[0].focus({ preventScroll: true });
      }
      if (!targets.every((t) => t.isConnected)) {
        clearCoach();
        return;
      }
      const next = ringBox(targets);
      setBox((prev) => (sameBox(prev, next) ? prev : next));
      frame = requestAnimationFrame(tick);
    };
    // The first look waits a frame, so a menu that opens with the ring
    // has taken focus before the ring moves it to the row.
    frame = requestAnimationFrame(tick);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') clearCoach();
    };
    // Capture, so the pick and the press that closes a menu both reach
    // the ring before the page acts on them, and neither is held back.
    document.addEventListener('pointerdown', clearCoach, true);
    document.addEventListener('keydown', onKey, true);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener('pointerdown', clearCoach, true);
      document.removeEventListener('keydown', onKey, true);
    };
  }, [shown]);

  useLayoutEffect(() => {
    const el = tipRef.current;
    if (!box || !el) return;
    const next = tipPlace(
      box,
      { width: el.offsetWidth, height: el.offsetHeight },
      { width: window.innerWidth, height: window.innerHeight },
    );
    setTip((prev) => (prev && prev.left === next.left && prev.top === next.top ? prev : next));
  }, [box]);

  if (!box) return null;
  return (
    <>
      <div className="cm-ring" aria-hidden="true" style={box} />
      <div
        ref={tipRef}
        className="cm-tip ov-toast"
        role="status"
        style={tip ?? { left: 0, top: 0, visibility: 'hidden' }}
      >
        <span className="ov-toast-msg">{shown.line}</span>
      </div>
    </>
  );
}
