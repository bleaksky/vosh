import { useEffect, type MouseEvent } from 'react';

interface WindowFocusArgs {
  focusInput: () => void;
  middleClick: (event: MouseEvent) => void;
}

// The paths that put the caret back on the command line: a click in the
// window, the window coming to the front and a copy. Returns the mouseup
// handlers for the terminal area and the main element.
export function useWindowFocus({ focusInput, middleClick }: WindowFocusArgs) {
  // Click anywhere in the terminal area focuses the input. Skip when
  // the user is selecting text (so copy still works) or clicking an
  // actual interactive element. A middle click goes to the split.
  const handleTerminalMouseUp = (event: MouseEvent<HTMLDivElement>) => {
    if (event.button === 1) {
      middleClick(event);
      return;
    }
    focusInputFromClick(event);
  };

  // Wider click handler attached to the <main>. Catches clicks outside
  // the terminal (panels, chrome) so the user who clicks anywhere in
  // the window — including after pulling focus back from Discord —
  // lands with the command line ready to type.
  const handleAppMouseUp = (event: MouseEvent<HTMLElement>) => {
    focusInputFromClick(event);
  };

  // Shared "click anywhere focuses input" logic. Skips interactive
  // elements (so the actual click handler runs and keeps its own
  // focus state) and selection drags (so copy still works).
  const focusInputFromClick = (event: MouseEvent<Element>) => {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement;
    // Floating surfaces and the panel edge keep the focus they hold, so
    // a press on a form's label or padding does not yank the caret out.
    if (
      target.closest(
        'button, input, textarea, select, a, label, [role="button"], [role="menu"], [role="dialog"], [role="separator"]',
      )
    )
      return;
    const selection = window.getSelection?.();
    if (selection && selection.toString().length > 0) return;
    focusInput();
  };

  // Tauri reports a window-level focus event when the OS brings the
  // app back to front (user clicked the Vosh window while it was
  // unfocused, or alt-tabbed in). Focusing the input here is the
  // "click-to-type" affordance the user expects on every reactivation.
  useEffect(() => {
    window.addEventListener('focus', focusInput);
    return () => window.removeEventListener('focus', focusInput);
    // focusInput reaches the command line through a ref, so the first one
    // the window hands in stays good for its life.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Mark the root while the window is in the background, so frame.css
  // can dim the window title to the tertiary tone the way the OS dims
  // an inactive title bar.
  useEffect(() => {
    const root = document.documentElement;
    const mark = () => {
      root.dataset.windowFocus = document.hasFocus() ? 'focused' : 'unfocused';
    };
    mark();
    window.addEventListener('focus', mark);
    window.addEventListener('blur', mark);
    return () => {
      window.removeEventListener('focus', mark);
      window.removeEventListener('blur', mark);
      delete root.dataset.windowFocus;
    };
  }, []);

  // After a copy the caret should land back on the command line. The
  // terminal copy path dispatches `vosh:focus-input` explicitly; the
  // DOM `copy` listener is the catch-all for a browser-native copy of
  // selected terminal text. Both skip when the copy came from a field
  // (the command input, the find box) so that field keeps its focus,
  // and the copy listener defers a frame so the clipboard reads the
  // selection before focus moves off it.
  useEffect(() => {
    const onCopy = () => {
      if ((document.activeElement as HTMLElement | null)?.closest('.input-row, input, textarea'))
        return;
      window.setTimeout(focusInput, 0);
    };
    window.addEventListener('vosh:focus-input', focusInput);
    document.addEventListener('copy', onCopy);
    return () => {
      window.removeEventListener('vosh:focus-input', focusInput);
      document.removeEventListener('copy', onCopy);
    };
    // focusInput reaches the command line through a ref, so the first one
    // the window hands in stays good for its life.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return { handleTerminalMouseUp, handleAppMouseUp };
}
