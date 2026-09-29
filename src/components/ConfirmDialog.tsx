import { useEffect, useId, useRef } from 'react';
import { useEscape } from '../lib/escapeStack';

interface Props {
  title: string;
  body: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}

// Destructive-action confirm on the floating recipe (SPEC 3): a 320
// wide card, radius 16, the title at 15/20 semibold, quiet body copy,
// and right-aligned Cancel and danger buttons. No scrim. A clear layer
// behind the card still catches a press outside it, which cancels.
// Exists because Tauri webviews silently reject window.confirm().
// Focus starts on Cancel. Esc cancels through the escape stack, so it
// closes this dialog and nothing under it. Enter presses the focused
// button, so Enter on Cancel cancels, and anywhere else in the window
// Enter confirms. That capture-phase keydown stops the key from
// leaking into inputs behind the card. The card can overlap the
// terminal, so the layer opts in to hiding the native surface via
// data-occludes-surface.
export function ConfirmDialog({ title, body, confirmLabel, onConfirm, onCancel }: Props) {
  const titleId = useId();
  const bodyId = useId();
  const cancelRef = useRef<HTMLButtonElement | null>(null);
  const cardRef = useRef<HTMLDivElement | null>(null);

  useEscape(true, onCancel);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Enter') return;
      if (e.target instanceof HTMLButtonElement && cardRef.current?.contains(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
      onConfirm();
    };
    document.addEventListener('keydown', onKey, true);
    return () => document.removeEventListener('keydown', onKey, true);
  }, [onConfirm]);

  // Start on Cancel, and hand focus back when the dialog closes.
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    cancelRef.current?.focus({ preventScroll: true });
    return () => {
      const current = document.activeElement;
      if (!current || current === document.body) previous?.focus();
    };
  }, []);

  return (
    <div
      className="ov-confirm-layer"
      data-occludes-surface="true"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
      onMouseUp={(e) => e.stopPropagation()}
    >
      <div
        ref={cardRef}
        className="ov-confirm"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
      >
        <h2 id={titleId} className="ov-confirm-title">
          {title}
        </h2>
        <p id={bodyId} className="ov-confirm-body">
          {body}
        </p>
        <div className="ov-confirm-actions">
          <button ref={cancelRef} type="button" className="ov-button" onClick={onCancel}>
            Cancel
          </button>
          <button type="button" className="ov-button is-danger" onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
