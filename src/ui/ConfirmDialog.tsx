import { useEffect, useId, useRef, type ReactNode } from 'react';
import { trapDialogFocus } from './dialogFocus';
import { cx } from './cx';
import { useEscape } from '../lib/escapeStack';

interface Props {
  title: string;
  body: string;
  confirmLabel: string;
  /** The cancel button's label, `Cancel` unless the choice reads better
   *  another way, like the banner ask's Not now. */
  cancelLabel?: string;
  /** The confirm button's look. `danger`, the default, for a choice
   *  that deletes or drops something, and `primary`, the accent fill,
   *  for one that makes something, like New plugin's Create. */
  tone?: 'primary' | 'danger';
  /** Holds the confirm button off, like Create while the name breaks
   *  its rule. Enter on the card confirms nothing meanwhile. */
  confirmDisabled?: boolean;
  /** Fields between the body and the buttons, like New plugin's Name. */
  children?: ReactNode;
  /** Where the card sits, from the window's right and bottom, for a
   *  confirm over the foot of the card that asks, like the writing
   *  card's Post…. It sits in the window's middle without one. */
  at?: { right: number; bottom: number };
  onConfirm: () => void;
  onCancel: () => void;
}

// A confirm on the floating recipe (SPEC 3): a 320 wide card, radius
// 16, the title at 15/20 semibold, quiet body copy, any fields, and
// right-aligned Cancel, or the label you name, and the confirm button, danger by default. No
// scrim. A clear layer behind the card still catches a press outside
// it, which cancels. Exists because Tauri webviews silently reject
// window.confirm().
// Focus starts on the first field, or on Cancel in a card with none,
// and stays inside the card (ui/dialogFocus). Tab cycles the card's
// controls, and focus that lands behind the card comes back to the
// first. Esc cancels through the escape stack, so it closes this dialog
// and nothing under it. Enter presses the focused button, so Enter on
// Cancel cancels. Enter on the card itself, after a press on its text,
// confirms. Enter from behind the card does nothing, so it never
// confirms and never reaches the control there.
export function ConfirmDialog({
  title,
  body,
  confirmLabel,
  cancelLabel = 'Cancel',
  tone = 'danger',
  confirmDisabled = false,
  children,
  at,
  onConfirm,
  onCancel,
}: Props) {
  const titleId = useId();
  const bodyId = useId();
  const cancelRef = useRef<HTMLButtonElement | null>(null);
  const cardRef = useRef<HTMLDivElement | null>(null);
  // Callers pass inline functions. The trap runs the latest one, or
  // none while the confirm button is off.
  const confirmRef = useRef<(() => void) | null>(onConfirm);
  useEffect(() => {
    confirmRef.current = confirmDisabled ? null : onConfirm;
  });

  useEscape(true, onCancel);

  // Start on the first field or on Cancel, hold focus in the card, and
  // hand focus back when the dialog closes. The trap lets go before
  // focus goes back, or it would pull the focus into the closing card
  // again.
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const card = cardRef.current;
    const release = card ? trapDialogFocus(document, card, () => confirmRef.current?.()) : () => {};
    const field = card?.querySelector<HTMLElement>('input, select, textarea');
    (field ?? cancelRef.current)?.focus({ preventScroll: true });
    return () => {
      release();
      const current = document.activeElement;
      if (!current || current === document.body) previous?.focus();
    };
  }, []);

  return (
    <div
      className={cx('ov-confirm-layer', at && 'is-placed')}
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
      onMouseUp={(e) => e.stopPropagation()}
    >
      <div
        ref={cardRef}
        className="ov-confirm"
        style={at ? { position: 'fixed', right: at.right, bottom: at.bottom } : undefined}
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
        {children}
        <div className="ov-confirm-actions">
          <button ref={cancelRef} type="button" className="ov-button" onClick={onCancel}>
            {cancelLabel}
          </button>
          <button
            type="button"
            className={cx('ov-button', tone === 'primary' ? 'is-primary' : 'is-danger')}
            disabled={confirmDisabled}
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
