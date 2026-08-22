import { useEffect } from 'react';

interface Props {
  title: string;
  body: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}

// Destructive-action confirm, from the Ember Menus canvas: dimmed
// scrim + 280px card with a Roboto Slab title, quiet body copy, and
// right-aligned cancel / danger chips. Exists because Tauri webviews
// silently reject window.confirm(). Escape or a scrim click cancels,
// Enter confirms. Capture-phase keydown + stopPropagation so the
// keys never leak into inputs behind the scrim, and preventDefault
// so a focused button does not also fire its click on Enter. The
// scrim can overlap the terminal, so it opts in to hiding the native
// surface via data-occludes-surface.
export function ConfirmDialog({ title, body, confirmLabel, onConfirm, onCancel }: Props) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onCancel();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        onConfirm();
      }
    };
    document.addEventListener('keydown', onKey, true);
    return () => document.removeEventListener('keydown', onKey, true);
  }, [onConfirm, onCancel]);

  return (
    <div
      className="confirm-backdrop"
      data-occludes-surface="true"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div className="confirm-card" role="dialog" aria-modal="true" aria-label={title}>
        <span className="confirm-title">{title}</span>
        <span className="confirm-body">{body}</span>
        <div className="confirm-actions">
          <button type="button" className="confirm-btn" onClick={onCancel}>
            cancel
          </button>
          <button type="button" className="confirm-btn confirm-btn-danger" onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
