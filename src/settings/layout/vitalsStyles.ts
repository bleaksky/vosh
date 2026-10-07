import type { UiFields, VitalsStyle } from '../../ipc/uiConfig';
import { VITALS_STYLES } from '../../ipc/uiConfig';
import type { VitalsSnapshot } from '../../ipc/vitals';
import { nextVitals, parseVitalsPacket, type Vitals } from '../../stores/gmcp/vitalsStore';

// What the Style gallery under Settings, Layout, Vitals works out, kept
// apart from VitalsGallery.tsx for its tests: the numbers its tiles
// draw, what a pick saves, where the arrow keys move it, the width and
// scale of a tile, and the caption under the tiles.

/** The narrowest a tile scales a footer, as a share of its width. */
const TILE_MIN_SCALE = 0.75;

/** The catalog's samples, Vosh's numbers while no session has yours. */
export const SAMPLE_VITALS: Vitals = nextVitals(null, {
  hp: 1020,
  maxhp: 1020,
  mana: 800,
  maxmana: 800,
  move: 930,
  maxmove: 930,
}) as Vitals;

/** The vitals the tiles draw, and whether they are your live ones. */
export interface GalleryVitals {
  vitals: Vitals;
  live: boolean;
}

export const OFFLINE: GalleryVitals = { vitals: SAMPLE_VITALS, live: false };

/** The vitals a snapshot holds, or the samples when it holds none. */
export function galleryVitals(snapshot: VitalsSnapshot | null): GalleryVitals {
  if (snapshot?.vitals == null) return OFFLINE;
  const packet = parseVitalsPacket(snapshot.vitals);
  const vitals = nextVitals(null, packet.values, packet.hidden);
  return vitals ? { vitals, live: true } : OFFLINE;
}

/** What a pick of `style` saves. Rows and One line live in
 *  vitals_density, so an older build still reads them, and clear any
 *  style you picked before (Q15). */
export function vitalsStylePick(style: VitalsStyle): UiFields {
  return style === 'rows' || style === 'line'
    ? { vitals_density: style, vitals_style: null }
    : { vitals_style: style };
}

const STEPS: Readonly<Record<string, 1 | -1>> = {
  ArrowRight: 1,
  ArrowDown: 1,
  ArrowLeft: -1,
  ArrowUp: -1,
};

/** The style an arrow `key` moves the pick to from `style`, around the
 *  ends as in any radio group, or null for any other key. */
export function arrowPick(key: string, style: VitalsStyle): VitalsStyle | null {
  const step = STEPS[key];
  if (step === undefined) return null;
  const n = VITALS_STYLES.length;
  return VITALS_STYLES[(VITALS_STYLES.indexOf(style) + step + n) % n];
}

/** What each style does, the caption's first sentence. */
const STYLE_CAPTIONS: Readonly<Record<VitalsStyle, string>> = {
  rows: 'Rows. Each vital gets a row of its own, its value at the right and a line under it.',
  line: 'One line. Health, Mana and Moves share one row and keep their lines.',
  ledger: 'Ledger. Each vital is a column, its name over a large figure and a line under it.',
  gauges:
    'Gauges. Each vital fills a pill between its label and its value, as the Group pane shows your group.',
  pips: 'Pips. Ten discs beside each value light up a tenth at a time, the way the moons light up the status line.',
  text: 'Text. You write your vitals with the codes your prompt uses, in the card you know from Customize prompt.',
};

/** The caption under the tiles: what `style` does, then the width the
 *  tiles draw at. */
export function galleryCaption(style: VitalsStyle, panel: number, drawn: number): string {
  const width =
    drawn < panel
      ? `Your panel is ${panel} pt wide, so each tile draws your vitals at ${Math.round(drawn)} pt, scaled to fit.`
      : `Each tile draws your vitals at your panel's width, ${panel} pt, scaled to fit.`;
  return `${STYLE_CAPTIONS[style]} ${width}`;
}

/** The width a footer draws at in a tile `tile` px wide, and the scale
 *  that fits it there. A tile not measured yet draws the panel's. */
export function tileFit(panel: number, tile: number): { width: number; scale: number } {
  if (!(tile > 0)) return { width: panel, scale: 1 };
  const width = Math.min(panel, tile / TILE_MIN_SCALE);
  return { width, scale: Math.min(1, tile / width) };
}
