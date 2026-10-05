import { WebglAddon } from '@xterm/addon-webgl';
import type { Terminal } from '@xterm/xterm';
import type { XtermBlink } from './xtermBlink';

/** Draws `term` with xterm's WebGL renderer when it can, and tells `blink`
 *  while WebGL draws. `quiet` is the history pane, which keeps the DOM
 *  renderer. `active` says whether WebGL still draws. */
export function loadWebgl(
  term: Terminal,
  blink: XtermBlink,
  quiet: boolean,
): { active(): boolean; dispose(): void } {
  // WebGL is on by default: the GPU renderer is far smoother for
  // scroll and burst output than xterm's DOM renderer. The webgl2
  // probe below still falls back to DOM when the WebView can't
  // allocate a context. Opt out with
  // `localStorage.setItem('vosh.webgl', '0')` (or the Settings
  // toggle), for the rare case where a context allocates but paints
  // nothing, which against Tauri's `transparent: true` window reads
  // as see-through desktop.
  let webglAddon: WebglAddon | null = null;
  const lsVal = typeof localStorage !== 'undefined' ? localStorage.getItem('vosh.webgl') : null;
  // On by default (opt-out). Only the live pane uses WebGL; the history
  // pane (quiet) always stays on the DOM renderer so the
  // split-scrollback overlay paints reliably.
  const enableWebgl = !quiet && lsVal !== '0';
  if (enableWebgl) {
    // Probe webgl2 in a throwaway canvas first. If the WebView
    // can't allocate a context, the addon would load, immediately
    // fire onContextLoss, and (per xterm 5.5.0 bug) leave the
    // terminal renderer in an unrenderable state. Cheaper to
    // detect now and stay on DOM.
    let probeOk = false;
    try {
      const probe = document.createElement('canvas');
      const ctx = probe.getContext('webgl2') as WebGL2RenderingContext | null;
      if (ctx) {
        probeOk = true;
        try {
          ctx.getExtension('WEBGL_lose_context')?.loseContext();
        } catch {
          // ignore probe cleanup failure
        }
      }
    } catch {
      probeOk = false;
    }
    if (!probeOk) {
      console.log('[vosh] webgl2 unavailable in this WebView, staying on DOM');
    }
    try {
      if (!probeOk) throw new Error('webgl2 probe failed');
      // xterm 6.1.0-beta's WebglAddon takes an options object and
      // also ships PR #5529 — synchronous redraw on resize — which
      // is what fixes the 1px-canvas-width oscillation that made
      // the divider drag wobble on the previous (5.5.0) line.
      // No options needed; the defaults match what we want.
      const addon = new WebglAddon();
      // On context loss (GPU reset, sleep/wake, too many live GL
      // contexts) dispose the addon so xterm hands rendering back to
      // its DOM renderer. Without this the pane keeps a dead GL
      // canvas, which against Tauri's transparent window reads as a
      // blank see-through hole until the user reloads — worse now
      // that WebGL is the default. Disposing is xterm's documented
      // context-loss fallback. The crash that blocked this on the
      // 5.5.0 line was a fit() racing a half-disposed renderer
      // (unguarded `_renderer.value.dimensions`); we sidestep that
      // regardless of version by deferring the repaint to the next
      // frame, so nothing touches the renderer in the same tick as
      // the swap inside dispose().
      addon.onContextLoss(() => {
        console.warn('[vosh] webgl context lost — falling back to DOM renderer');
        try {
          addon.dispose();
        } catch (err) {
          console.warn('[vosh] webgl dispose after context loss failed', err);
        }
        webglAddon = null;
        blink.setWebgl(false);
        // Force the DOM renderer to paint the visible rows once the
        // swap settles. The buffer is untouched by the renderer
        // change, so this just makes the content reappear immediately
        // instead of on the next server write.
        requestAnimationFrame(() => {
          try {
            term.refresh(0, term.rows - 1);
          } catch {
            // ignore — the DOM renderer repaints on the next write
          }
        });
      });
      term.loadAddon(addon);
      webglAddon = addon;
      blink.setWebgl(true);
      console.log('[vosh] webgl renderer active');
    } catch (err) {
      console.log('[vosh] webgl renderer failed, staying on DOM', err);
      webglAddon = null;
    }
  } else {
    console.log('[vosh] webgl off — re-enable with localStorage.removeItem("vosh.webgl")');
  }
  return {
    active: () => webglAddon !== null,
    dispose: () => {
      // Forget the addon first, so active() reads false even when its
      // dispose throws.
      const addon = webglAddon;
      webglAddon = null;
      addon?.dispose();
    },
  };
}
