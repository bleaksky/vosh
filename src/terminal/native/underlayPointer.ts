import { nativeSurfacePointer, nativeSurfaceWheel } from '../../ipc/nativeSurface';
import { nativeSurfaceEnabled } from '../terminalRenderer';

// Underlay input. The webview sits above the surface and receives every
// pointer event over the pane, so forward them to the native grid in
// pane-local CSS px. Moves coalesce to one IPC per frame and read the
// pane rect once per frame. A release flushes the last move first so a
// drag ends where the pointer did. preventDefault on press keeps focus
// in the command line and stops the page from starting a text
// selection. It cannot shield an IME composition, which WebKit hands
// every mouse event first. Right click and Control click fall through
// to the terminal menu's contextmenu handler.
//
// `spare` gives the pixels the grid starts below the pane's top. It is
// read on each event, since the bounds report sets it again on every sync.
// It returns the function that detaches the listeners, or undefined when
// the native grid does not draw this pane.
export function forwardUnderlayPointer(
  sizer: HTMLElement | null,
  quiet: boolean,
  spare: () => number,
): (() => void) | undefined {
  if (quiet || !nativeSurfaceEnabled() || !sizer) return undefined;
  const send = (kind: string, x: number, y: number, open: boolean) => {
    void nativeSurfacePointer({ kind, x, y, open }).catch(() => {});
  };
  const toLocal = (clientX: number, clientY: number) => {
    const r = sizer.getBoundingClientRect();
    return { x: clientX - r.left, y: clientY - r.top - spare() };
  };
  let dragging = false;
  let last = { clientX: 0, clientY: 0 };
  let pending = false;
  let raf = 0;
  const flush = () => {
    raf = 0;
    if (!pending) return;
    pending = false;
    const p = toLocal(last.clientX, last.clientY);
    send(dragging ? 'drag' : 'move', p.x, p.y, false);
  };
  // End a drag however it ends: a release, a cancel, lost capture, or a
  // move with the button already up because the release went elsewhere.
  const release = (pointerId?: number) => {
    if (!dragging) return;
    if (raf) cancelAnimationFrame(raf);
    flush();
    dragging = false;
    if (pointerId !== undefined && sizer.hasPointerCapture(pointerId)) {
      sizer.releasePointerCapture(pointerId);
    }
    const p = toLocal(last.clientX, last.clientY);
    send('up', p.x, p.y, false);
  };
  const onDown = (e: PointerEvent) => {
    last = { clientX: e.clientX, clientY: e.clientY };
    if (e.button === 1) {
      e.preventDefault();
      const p = toLocal(e.clientX, e.clientY);
      send('middle', p.x, p.y, false);
      return;
    }
    if (e.button === 2) {
      // Keep the caret in the command line. contextmenu still fires.
      e.preventDefault();
      return;
    }
    // Control click is the macOS context click. Leave it to the menu.
    if (e.button !== 0 || e.ctrlKey) return;
    e.preventDefault();
    try {
      sizer.setPointerCapture(e.pointerId);
    } catch {
      // No active pointer to capture (a synthetic event). The drag
      // still works while the pointer stays over the pane.
    }
    dragging = true;
    const p = toLocal(e.clientX, e.clientY);
    send('down', p.x, p.y, e.metaKey);
  };
  const onMove = (e: PointerEvent) => {
    last = { clientX: e.clientX, clientY: e.clientY };
    if (dragging && (e.buttons & 1) === 0) {
      release(e.pointerId);
      return;
    }
    pending = true;
    if (!raf) raf = requestAnimationFrame(flush);
  };
  const onUp = (e: PointerEvent) => {
    last = { clientX: e.clientX, clientY: e.clientY };
    if ((e.buttons & 1) === 0) release(e.pointerId);
  };
  const onCancel = (e: PointerEvent) => release(e.pointerId);
  const onLeave = () => {
    if (dragging) return;
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
    pending = false;
    send('leave', 0, 0, false);
  };
  // WebKit reports wheel deltas with the opposite sign of AppKit's
  // scrollingDeltaY (positive deltaY scrolls toward newer output), and
  // the backend accumulator is tuned for AppKit pixels. WebKit on macOS
  // always sends pixel mode. Line and page modes scale up only in case
  // another engine sends them.
  const onWheel = (e: WheelEvent) => {
    const scale = e.deltaMode === 1 ? 8.5 : e.deltaMode === 2 ? 200 : 1;
    const delta = -e.deltaY * scale;
    if (delta === 0) return;
    void nativeSurfaceWheel(delta).catch(() => {});
  };
  sizer.addEventListener('pointerdown', onDown);
  sizer.addEventListener('pointermove', onMove);
  sizer.addEventListener('pointerup', onUp);
  sizer.addEventListener('pointercancel', onCancel);
  sizer.addEventListener('lostpointercapture', onCancel);
  sizer.addEventListener('pointerleave', onLeave);
  sizer.addEventListener('wheel', onWheel, { passive: true });
  return () => {
    if (raf) cancelAnimationFrame(raf);
    if (dragging) send('up', 0, 0, false);
    send('leave', 0, 0, false);
    sizer.removeEventListener('pointerdown', onDown);
    sizer.removeEventListener('pointermove', onMove);
    sizer.removeEventListener('pointerup', onUp);
    sizer.removeEventListener('pointercancel', onCancel);
    sizer.removeEventListener('lostpointercapture', onCancel);
    sizer.removeEventListener('pointerleave', onLeave);
    sizer.removeEventListener('wheel', onWheel);
  };
}
