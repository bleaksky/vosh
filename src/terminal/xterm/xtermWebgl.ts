import { WebglAddon } from '@xterm/addon-webgl';
import type { Terminal } from '@xterm/xterm';
import type { XtermBlink } from './xtermBlink';

/** xterm's WebGL renderer for one pane. Only the pane that shows holds
 *  it: `load` gives it the renderer as it shows, after its first fit,
 *  and `release` hands it back to xterm's DOM renderer as it hides. A
 *  window keeps a terminal for each session it opened, and WebView2
 *  allows about 16 live GL contexts. */
export interface XtermWebgl {
  load(): void;
  release(): void;
  /** The pane is going. Let WebGL go and paint nothing after. */
  dispose(): void;
}

/** Draws `term` with xterm's WebGL renderer while it shows, when it can,
 *  and tells `blink` while WebGL draws. `quiet` is the history pane,
 *  which keeps the DOM renderer. */
export function xtermWebgl(term: Terminal, blink: XtermBlink, quiet: boolean): XtermWebgl {
  // WebGL is on by default: the GPU renderer is far smoother for
  // scroll and burst output than xterm's DOM renderer. The webgl2
  // probe below still falls back to DOM when the WebView can't
  // allocate a context. Opt out with
  // `localStorage.setItem('vosh.webgl', '0')` (or the Settings
  // toggle), for the rare case where a context allocates but paints
  // nothing, which against Tauri's `transparent: true` window reads
  // as see-through desktop.
  let webglAddon: WebglAddon | null = null;
  // The pane went, and xterm with it.
  let disposed = false;
  const lsVal = typeof localStorage !== 'undefined' ? localStorage.getItem('vosh.webgl') : null;
  // On by default (opt-out). Only the live pane uses WebGL; the history
  // pane (quiet) always stays on the DOM renderer so the
  // split-scrollback overlay paints reliably.
  const enableWebgl = !quiet && lsVal !== '0';
  if (!enableWebgl) {
    console.log('[vosh] webgl off — re-enable with localStorage.removeItem("vosh.webgl")');
  }

  const load = () => {
    if (disposed || !enableWebgl || webglAddon) return;
    // Probe webgl2 in a throwaway canvas first. If the WebView
    // can't allocate a context, the addon would load and fire
    // onContextLoss at once, a renderer swap that can leave the pane
    // unable to draw. Finding out first keeps it on DOM.
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
      // The defaults suit the pane. This WebglAddon redraws in step
      // with a resize, which keeps the canvas from wobbling a pixel
      // wide as you drag the split's divider.
      const addon = new WebglAddon();
      // On context loss (GPU reset, sleep/wake, too many live GL
      // contexts) dispose the addon so xterm hands rendering back to
      // its DOM renderer, xterm's documented context-loss fallback.
      // Without this the pane keeps a dead GL canvas, which against
      // Tauri's transparent window reads as a blank see-through hole
      // until you reload. The repaint waits for the next frame, since
      // a fit() in the same tick as the swap inside dispose() reads a
      // half-disposed renderer (`_renderer.value.dimensions`).
      addon.onContextLoss(() => {
        console.warn('[vosh] webgl context lost — falling back to DOM renderer');
        try {
          addon.dispose();
        } catch (err) {
          console.warn('[vosh] webgl dispose after context loss failed', err);
        }
        if (webglAddon === addon) webglAddon = null;
        blink.setWebgl(false);
        // Force the DOM renderer to paint the visible rows once the
        // swap settles. The buffer is untouched by the renderer
        // change, so this just makes the content reappear immediately
        // instead of on the next server write.
        requestAnimationFrame(() => {
          // A pane that went in the meantime has no renderer to paint,
          // and a refresh of it queues a frame that throws.
          if (disposed) return;
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
  };

  const release = () => {
    const addon = webglAddon;
    if (!addon) return;
    webglAddon = null;
    blink.setWebgl(false);
    // WebglAddon's dispose reads `_terminal._core._store._isDisposed`
    // and throws when xterm has already torn down its core, as it has
    // when the pane unmounts. The renderer still releases its GL
    // resources before the throw, so there is nothing more to do.
    try {
      addon.dispose();
    } catch {
      // intentional swallow, see above
    }
  };

  const dispose = () => {
    disposed = true;
    release();
  };

  return { load, release, dispose };
}
