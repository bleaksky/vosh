import { useSyncExternalStore } from 'react';

// The face and the size the panes and the status line draw at, for the
// code that measures their text or draws it on a canvas: the Grouped
// chips, the vitals on one line, and the map. CSS gives every pane and
// the status line var(--font-panel) (tokens.css) at --panel-text-px, so
// what is read here off the root is what draws. The main window writes
// both on the root's style when your Panel text or your terminal font
// changes.
//
// A canvas measures and draws a face that has not loaded yet in its
// fallback, and the faces Vosh bundles load with font-display: block
// after the first paint, as do the installed fonts the font scheme
// serves. So the version counts up each time a face finishes loading
// and each time the panel face or size changes, and whatever measured
// or drew with it does so again.

let version = 0;
let started = false;
let seen = '';
const listeners = new Set<() => void>();

/** The panel face as a CSS font list, read off the root. */
export function readPanelFace(): string {
  if (typeof document === 'undefined') return 'ui-monospace, monospace';
  const value = getComputedStyle(document.documentElement).getPropertyValue('--font-panel');
  return value.trim() || 'ui-monospace, monospace';
}

/** The panel size in px, read off the root. 12, the size the panes were
 *  drawn at, with nothing there. */
export function readPanelTextPx(): number {
  if (typeof document === 'undefined') return 12;
  const value = getComputedStyle(document.documentElement).getPropertyValue('--panel-text-px');
  const px = Number(value.trim() || NaN);
  return Number.isFinite(px) && px > 0 ? px : 12;
}

// The face and the size together, so a change to either one counts.
function readPanelText(): string {
  return `${readPanelFace()}|${readPanelTextPx()}`;
}

function bump(): void {
  version += 1;
  for (const cb of listeners) cb();
}

function start(): void {
  if (started || typeof document === 'undefined') return;
  started = true;
  seen = readPanelText();
  document.fonts?.addEventListener('loadingdone', bump);
  new MutationObserver(() => {
    const next = readPanelText();
    if (next === seen) return;
    seen = next;
    bump();
  }).observe(document.documentElement, { attributes: true, attributeFilter: ['style'] });
}

/** Hear the panel face or size change or a face finish loading. */
export function subscribePanelFace(cb: () => void): () => void {
  start();
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/** Counts up each time the panel face or size changes or a face
 *  loads. */
export function panelFaceVersion(): number {
  return version;
}

/** The panel text's version, so a measure keyed on it runs again. */
export function usePanelFaceVersion(): number {
  return useSyncExternalStore(subscribePanelFace, panelFaceVersion, panelFaceVersion);
}
