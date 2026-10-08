// The accent block that stands in for the command line's own caret.

import { useEffect, useRef, useState, type RefObject } from 'react';

/** Place the block caret for the command line in `inputRef`, which holds
 *  `value`. Input draws the mirror and the caret with the refs this
 *  returns, at caretPos, and calls measureCaret on focus, blur and
 *  scroll. */
export function useCaret(
  inputRef: RefObject<HTMLInputElement | HTMLTextAreaElement>,
  value: string,
) {
  // The block caret. The textarea's native caret is transparent in
  // CSS and replaced with an absolutely-positioned accent block. The
  // position comes from a hidden mirror div cloning the textarea's
  // text up to selectionStart with identical font, padding, and
  // wrapping — no DOM API reports a textarea caret rect directly, so
  // a marker span appended after the cloned text stands in for it.
  // Null hides the block: blurred, password mode (the masked input
  // keeps its native caret), or a non-collapsed selection (the OS
  // paints the selection highlight instead).
  const mirrorRef = useRef<HTMLDivElement | null>(null);
  const caretRef = useRef<HTMLSpanElement | null>(null);
  const [caretPos, setCaretPos] = useState<{ left: number; top: number } | null>(null);

  const measureCaret = () => {
    const el = inputRef.current;
    const mirror = mirrorRef.current;
    if (
      !mirror ||
      !(el instanceof HTMLTextAreaElement) ||
      document.activeElement !== el ||
      el.selectionStart === null ||
      el.selectionStart !== el.selectionEnd
    ) {
      setCaretPos(null);
      return;
    }
    // Rebuild the mirror: text up to the caret (line breaks survive —
    // the mirror is pre-wrap), then the marker. The zero-width space
    // gives the marker the line box's height so the 15px block can
    // center on the line, and cloning the textarea's clientWidth keeps
    // the soft-wrap points identical.
    mirror.style.width = `${el.clientWidth}px`;
    mirror.textContent = el.value.slice(0, el.selectionStart);
    const marker = document.createElement('span');
    marker.textContent = '\u200b';
    mirror.appendChild(marker);
    const left = el.offsetLeft + marker.offsetLeft - el.scrollLeft;
    const top = el.offsetTop + marker.offsetTop - el.scrollTop + (marker.offsetHeight - 15) / 2;
    setCaretPos((prev) => (prev && prev.left === left && prev.top === top ? prev : { left, top }));
  };
  // Latest-closure ref so the document-level listeners below register
  // exactly once instead of resubscribing every render.
  const measureCaretRef = useRef(measureCaret);
  measureCaretRef.current = measureCaret;

  // Re-measure after every commit (edits, the password/textarea swap,
  // programmatic setSelectionRange). The equality guard in setCaretPos
  // keeps the follow-up render from looping.
  useEffect(() => {
    measureCaretRef.current();
  });

  // Caret moves that change no React state (arrow keys, clicks) only
  // surface as document selectionchange; a window resize reflows the
  // row and shifts the wrap points.
  useEffect(() => {
    const remeasure = () => measureCaretRef.current();
    document.addEventListener('selectionchange', remeasure);
    window.addEventListener('resize', remeasure);
    return () => {
      document.removeEventListener('selectionchange', remeasure);
      window.removeEventListener('resize', remeasure);
    };
  }, []);

  // Hold the caret solid while typing: restarting the blink animation
  // on every edit puts it back at the visible half of its keyframe.
  useEffect(() => {
    const el = caretRef.current;
    if (!el) return;
    el.style.animation = 'none';
    void el.offsetWidth;
    el.style.animation = '';
  }, [value]);

  return { mirrorRef, caretRef, caretPos, measureCaret };
}
