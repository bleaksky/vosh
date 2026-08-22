import { useEffect, useState } from 'react';
import { dismissToast, getToasts, subscribeToasts, type Toast } from '../lib/toasts';

// Bottom-right toast stack, from the Ember Menus canvas: 340px rows
// on the shared floating-surface recipe, anchored at the update
// notice's corner (right 16 / bottom 40, above the status strip).
// Success leads with a check, error tints the border and carries a
// danger dot, info a neutral info dot. The store owns the dismiss
// timers; clicking a toast dismisses it early. Like .update-notice
// these sit over the bottom chrome rows rather than the terminal,
// so no data-occludes-surface.
export function Toasts() {
  const [toasts, setToasts] = useState<Toast[]>(getToasts);

  useEffect(() => subscribeToasts(setToasts), []);

  if (toasts.length === 0) return null;

  return (
    <div className="toast-stack" role="status" aria-live="polite">
      {toasts.map((t) => (
        <button
          key={t.id}
          type="button"
          className={`toast toast-${t.kind}`}
          title="dismiss"
          onClick={() => dismissToast(t.id)}
        >
          {t.kind === 'success' ? (
            <svg
              className="toast-check"
              width="13"
              height="13"
              viewBox="0 0 13 13"
              fill="none"
              aria-hidden="true"
            >
              <path d="M2.2 6.9 5.2 9.9 10.8 3.6" />
            </svg>
          ) : (
            <span className="toast-dot" aria-hidden="true" />
          )}
          <span className="toast-msg">{t.message}</span>
          {t.meta && <span className="toast-meta">{t.meta}</span>}
        </button>
      ))}
    </div>
  );
}
