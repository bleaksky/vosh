import { useMemo, useSyncExternalStore } from 'react';
import { FIXED_MEASURE, type ChipMeasure } from './chipsGrid';

// Text widths for the Grouped chips packer, in the faces the pane draws:
// the live terminal face (--font-mud, which follows your font setting)
// for names and hours, and the UI face (--font-ui) for the group names
// and the count. The pane and its minimum share this one measure.
//
// The terminal faces Vosh bundles load with font-display: block, after
// the first paint, and a canvas measures a face that has not loaded in
// its fallback. So the widths are dropped and measured again whenever a
// face finishes loading, and whenever the faces themselves change (a
// font setting, which the main window writes on the root's style).

let version = 0;
let started = false;
let facesKey = '';
const listeners = new Set<() => void>();

interface Faces {
  mono: string;
  ui: string;
}

function readFaces(): Faces {
  if (typeof document === 'undefined') return { mono: 'monospace', ui: 'sans-serif' };
  const root = getComputedStyle(document.documentElement);
  return {
    mono: root.getPropertyValue('--font-mud').trim() || 'ui-monospace, monospace',
    ui: root.getPropertyValue('--font-ui').trim() || 'system-ui, sans-serif',
  };
}

function bump(): void {
  version += 1;
  for (const cb of listeners) cb();
}

function start(): void {
  if (started || typeof document === 'undefined') return;
  started = true;
  facesKey = JSON.stringify(readFaces());
  document.fonts?.addEventListener('loadingdone', bump);
  // The main window sets --app-font-family on the root when your font
  // setting changes, and a theme paints the root the same way.
  new MutationObserver(() => {
    const next = JSON.stringify(readFaces());
    if (next === facesKey) return;
    facesKey = next;
    bump();
  }).observe(document.documentElement, { attributes: true, attributeFilter: ['style'] });
}

function subscribe(cb: () => void): () => void {
  start();
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

const getVersion = () => version;

let canvas: HTMLCanvasElement | null = null;

/** A measure over the faces as they are now, with its own cache. */
export function liveChipMeasure(): ChipMeasure {
  if (typeof document === 'undefined') return FIXED_MEASURE;
  const faces = readFaces();
  const cache = new Map<string, number>();
  const width = (font: string, text: string) => {
    const key = `${font}\u0000${text}`;
    const hit = cache.get(key);
    if (hit !== undefined) return hit;
    canvas ??= document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    if (!ctx) return text.length * 7.2;
    ctx.font = font;
    const w = ctx.measureText(text).width;
    cache.set(key, w);
    return w;
  };
  return {
    mono: (s) => width(`12px ${faces.mono}`, s),
    hours: (s) => width(`700 12px ${faces.mono}`, s),
    label: (s) => width(`600 11px ${faces.ui}`, s),
    count: (s) => width(`12px ${faces.ui}`, s),
  };
}

/** The live measure, new each time a face loads or changes, so the
 *  pane and its minimum pack again. */
export function useChipMeasure(): ChipMeasure {
  const v = useSyncExternalStore(subscribe, getVersion, getVersion);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(() => liveChipMeasure(), [v]);
}
