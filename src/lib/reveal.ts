// Showing a window once its first visible frame holds your theme.
//
// Every Vosh window starts hidden and shows itself after it applies the
// theme from the UI config. Showing it right after the apply can beat
// WebKit's repaint, and the first frame on screen is then the one from
// before the apply. The startup paint (theme/themePaint) usually has the
// theme on screen before React renders, and then the window can show at
// once. When it does not, from a first launch with no cache or a theme
// that changed some other way, the window waits for a frame with the
// theme to go out. WKWebView may run no animation frames in a hidden
// window, so a short timer backs the frames up.

import { paintMatchesBoot } from '../theme/theme';

/** The longest a window waits for a frame with the new theme. */
export const REPAINT_WAIT_MS = 100;

export interface RevealDeps {
  /** Whether the startup paint already shows the active theme. */
  painted: () => boolean;
  /** Run `fn` before the next frame, like requestAnimationFrame. */
  frame: (fn: () => void) => void;
  /** Run `fn` after `ms`. Returns a cancel. */
  wait: (fn: () => void, ms: number) => () => void;
}

const pageDeps: RevealDeps = {
  painted: paintMatchesBoot,
  frame: (fn) => {
    try {
      window.requestAnimationFrame(() => fn());
    } catch {
      // No animation frames here. The timer shows the window.
    }
  },
  wait: (fn, ms) => {
    const id = window.setTimeout(fn, ms);
    return () => window.clearTimeout(id);
  },
};

/** Call `show` once the window has painted the active theme: at once
 *  when the startup paint holds it, else after two animation frames
 *  (the second runs once a frame with the theme went out) or after
 *  REPAINT_WAIT_MS, whichever comes first. `show` runs once. */
export function showAfterThemePaint(show: () => void, deps: RevealDeps = pageDeps): void {
  if (deps.painted()) {
    show();
    return;
  }
  let done = false;
  let cancelWait: () => void = () => {};
  const finish = () => {
    if (done) return;
    done = true;
    cancelWait();
    show();
  };
  cancelWait = deps.wait(finish, REPAINT_WAIT_MS);
  deps.frame(() => deps.frame(finish));
}
