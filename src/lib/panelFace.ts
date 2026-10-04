import { useSyncExternalStore } from 'react';

// The face the panes and the status line draw in, for the code that
// measures their text or draws it on a canvas: the Grouped chips, the
// vitals on one line, and the map. CSS gives every pane and the status
// line var(--font-panel) (tokens.css), so the face read here off the
// root is the face that draws. The main window writes it on the root's
// style when your Panel font or your terminal font changes.
//
// A canvas measures and draws a face that has not loaded yet in its
// fallback, and the faces Vosh bundles load with font-display: block
// after the first paint, as do the installed fonts the font scheme
// serves. So the version counts up each time a face finishes loading
// and each time the panel face changes, and whatever measured or drew
// with it does so again.

let version = 0;
let started = false;
let face = '';
const listeners = new Set<() => void>();

/** The panel face as a CSS font list, read off the root. */
export function readPanelFace(): string {
  if (typeof document === 'undefined') return 'ui-monospace, monospace';
  const value = getComputedStyle(document.documentElement).getPropertyValue('--font-panel');
  return value.trim() || 'ui-monospace, monospace';
}

function bump(): void {
  version += 1;
  for (const cb of listeners) cb();
}

function start(): void {
  if (started || typeof document === 'undefined') return;
  started = true;
  face = readPanelFace();
  document.fonts?.addEventListener('loadingdone', bump);
  new MutationObserver(() => {
    const next = readPanelFace();
    if (next === face) return;
    face = next;
    bump();
  }).observe(document.documentElement, { attributes: true, attributeFilter: ['style'] });
}

/** Hear the panel face change or a face finish loading. */
export function subscribePanelFace(cb: () => void): () => void {
  start();
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/** Counts up each time the panel face changes or a face loads. */
export function panelFaceVersion(): number {
  return version;
}

/** The panel face's version, so a measure keyed on it runs again. */
export function usePanelFaceVersion(): number {
  return useSyncExternalStore(subscribePanelFace, panelFaceVersion, panelFaceVersion);
}
