import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { dismissToast, getToasts, pushToast, subscribeToasts, type Toast } from '../lib/toasts';

/** How long the copy confirmation stays up. Matches the native toast
 *  it replaces. */
const COPY_TOAST_MS = 1600;

let copyListening = false;

// Under the underlay the native renderer reports a selection copy as
// `vosh://native-copied` with the character count, and the page shows
// the confirmation. Started once at module scope so a remount (or a
// second mount) never doubles the toast.
function startCopyToasts() {
  if (copyListening) return;
  copyListening = true;
  listen<unknown>('vosh://native-copied', (event) => {
    const chars = Number(event.payload);
    if (!Number.isFinite(chars) || chars <= 0) return;
    pushToast({
      kind: 'success',
      message: `Copied ${chars.toLocaleString()} ${chars === 1 ? 'character' : 'characters'}`,
      timeoutMs: COPY_TOAST_MS,
    });
  }).catch(() => {
    // Outside Tauri there is no event bus. Allow a later mount to retry.
    copyListening = false;
  });
}

// Toast stack at the bottom right of the terminal column, 16 in from
// its right edge and 16 above the input band. Each toast is a floating
// card (radius 16, the SPEC 3 recipe) with a leading status mark: a
// check in the success color, or a dot in danger for errors and in the
// accent for info. The store owns the dismiss timers, and clicking a
// toast dismisses it early. It works inside the positioned terminal
// area or as a direct child of the shell grid, where overlays.css pins
// it to the terminal cell. The stack floats over the terminal, so it
// carries data-occludes-surface for the Windows and Linux on-top
// native path.
export function Toasts() {
  const [toasts, setToasts] = useState<Toast[]>(getToasts);

  useEffect(() => subscribeToasts(setToasts), []);
  useEffect(startCopyToasts, []);

  if (toasts.length === 0) return null;

  return (
    <div className="ov-toasts" role="status" aria-live="polite" data-occludes-surface="true">
      {toasts.map((t) => (
        <button
          key={t.id}
          type="button"
          className={`ov-toast is-${t.kind}`}
          title="Dismiss"
          onClick={() => dismissToast(t.id)}
        >
          {t.kind === 'success' ? (
            <svg
              className="ov-toast-check"
              width="16"
              height="16"
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.25"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <path d="M3.5 8.5l3 3 6-7" />
            </svg>
          ) : (
            <span className="ov-toast-dot" aria-hidden="true" />
          )}
          <span className="ov-toast-msg">{t.message}</span>
          {t.meta && (
            <span className={t.metaMono ? 'ov-toast-meta is-mono' : 'ov-toast-meta'}>{t.meta}</span>
          )}
        </button>
      ))}
    </div>
  );
}
