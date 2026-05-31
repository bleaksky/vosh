import { useEffect } from 'react';

// Browser-level "Are you sure you want to leave?" prompt when the
// tab/window would close with dirty fields. Tauri webviews honor
// the standard beforeunload, so this also covers the Settings
// window's close button. Returns early when not dirty so we don't
// keep registering no-op listeners on every render.
export function useUnsavedWarning(dirty: boolean): void {
  useEffect(() => {
    if (!dirty) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      // Some browsers ignore the custom string but still respect
      // the returnValue assignment, so set both.
      e.returnValue = '';
      return '';
    };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  }, [dirty]);
}
