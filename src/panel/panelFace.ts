import { useSyncExternalStore } from 'react';

// The faces and the size the panes and the status line draw at, for the
// code that measures their text or draws it on a canvas: the Grouped
// chips, the vitals on one line, and the map. CSS gives every pane and
// the status line the panel faces (tokens.css) at --panel-text-px, so
// what is read here off the root is what draws. The main window writes
// your Panel text picks on the root's style, and your terminal font,
// which the game face draws in under As designed.
//
// A canvas measures and draws a face that has not loaded yet in its
// fallback, and the faces Vosh bundles load with font-display: block
// after the first paint, as do the installed fonts the font scheme
// serves. So the version counts up each time a face finishes loading
// and each time a panel face or the size changes, and whatever measured
// or drew with it does so again.

let version = 0;
let started = false;
let seen = '';
const listeners = new Set<() => void>();

function readRoot(name: string, fallback: string): string {
  if (typeof document === 'undefined') return fallback;
  const value = getComputedStyle(document.documentElement).getPropertyValue(name);
  return value.trim() || fallback;
}

/** The panel face as a CSS font list, read off the root: the headers,
 *  labels, counts and rows, the vitals, the status line, and the map's
 *  floor numbers. */
export function readPanelFace(): string {
  return readRoot('--font-panel', 'system-ui, sans-serif');
}

/** The game face as a CSS font list, read off the root: the affects,
 *  the countdown lines, the chips and chat. */
export function readPanelGameFace(): string {
  return readRoot('--font-panel-game', 'ui-monospace, monospace');
}

/** The face the map canvas draws its up and down marks and its notice
 *  in, read off the root. */
export function readPanelMarkFace(): string {
  return readRoot('--font-panel-mark', 'monospace');
}

/** The panel size in px, read off the root. 12, the size the panes were
 *  drawn at, with nothing there. */
export function readPanelTextPx(): number {
  const px = Number(readRoot('--panel-text-px', '') || NaN);
  return Number.isFinite(px) && px > 0 ? px : 12;
}

// The faces and the size together, so a change to any one counts.
function readPanelText(): string {
  return [readPanelFace(), readPanelGameFace(), readPanelMarkFace(), readPanelTextPx()].join('|');
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

/** Hear a panel face or the size change or a face finish loading. */
export function subscribePanelFace(cb: () => void): () => void {
  start();
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/** Counts up each time a panel face or the size changes or a face
 *  loads. */
export function panelFaceVersion(): number {
  return version;
}

/** The panel text's version, so a measure keyed on it runs again. */
export function usePanelFaceVersion(): number {
  return useSyncExternalStore(subscribePanelFace, panelFaceVersion, panelFaceVersion);
}

let measureCanvas: HTMLCanvasElement | null = null;
// Widths by font and text. The labels and maxes rarely change, so a
// vitals update reads these instead of measuring again.
const widths = new Map<string, number>();

/** How wide `text` draws in `font`. Values use tabular numbers, where
 *  every digit is as wide as a zero, so digits measure as zeros. A
 *  width taken before a face loaded is its fallback's, so `faceVersion`
 *  keys each width to the faces loaded when it was taken. */
export function textWidth(text: string, font: string, faceVersion: number): number {
  const shape = text.replace(/[0-9]/g, '0');
  const key = `${faceVersion}|${font}|${shape}`;
  const known = widths.get(key);
  if (known !== undefined) return known;
  measureCanvas ??= document.createElement('canvas');
  const ctx = measureCanvas.getContext('2d');
  if (!ctx) return shape.length * 7;
  ctx.font = font;
  const width = Math.ceil(ctx.measureText(shape).width);
  if (widths.size > 256) widths.clear();
  widths.set(key, width);
  return width;
}
