import { useEffect, useId, useRef } from 'react';
import { trapDialogFocus } from './dialogFocus';
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
// Focus starts on Cancel and stays inside the card (lib/dialogFocus).
// Tab cycles Cancel and the danger button, and focus that lands behind
// the card comes back to Cancel. Esc cancels through the escape stack,
// so it closes this dialog and nothing under it. Enter presses the
// focused button, so Enter on Cancel cancels. Enter on the card itself,
// after a press on its text, confirms. Enter from behind the card does
// nothing, so it never confirms and never reaches the control there.
export function ConfirmDialog({ title, body, confirmLabel, onConfirm, onCancel }: Props) {
  const titleId = useId();
  const bodyId = useId();
  const cancelRef = useRef<HTMLButtonElement | null>(null);
  const cardRef = useRef<HTMLDivElement | null>(null);
  // Callers pass inline functions. The trap runs the latest one.
  const confirmRef = useRef(onConfirm);
  useEffect(() => {
    confirmRef.current = onConfirm;
  });

  useEscape(true, onCancel);

  // Start on Cancel, hold focus in the card, and hand focus back when
  // the dialog closes. The trap lets go before focus goes back, or it
  // would pull the focus into the closing card again.
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const card = cardRef.current;
    const release = card ? trapDialogFocus(document, card, () => confirmRef.current()) : () => {};
    cancelRef.current?.focus({ preventScroll: true });
    return () => {
      release();
      const current = document.activeElement;
      if (!current || current === document.body) previous?.focus();
    };
  }, []);

  return (
    <div
      className="ov-confirm-layer"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
      onMouseUp={(e) => e.stopPropagation()}
    >
      <div
        ref={cardRef}
        className="ov-confirm"
        tabIndex={-1}
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
