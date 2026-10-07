import type { UiConfig, Vital, VitalsStyle } from '../../ipc/uiConfig';
import { DEFAULT_VITALS_CUSTOM, VITALS, VITALS_STYLES } from '../../ipc/uiConfig';
import { startVitalsText, type VitalsSnapshot } from '../../ipc/vitals';
import { VITAL_LABELS } from '../../panel/vitalsView';
import { nextVitals, parseVitalsPacket, type Vitals } from '../../stores/gmcp/vitalsStore';

// What the Style gallery under Settings, Layout, Vitals and the
// Customize vitals section under it work out, kept apart from their
// components for their tests: the numbers the tiles draw, what a pick
// saves, where the arrow keys move it, the width and scale of a tile,
// the caption under the tiles, and when Reset to default wakes.

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
  bands:
    'Bands. Each vital fills a bar over quiet bands that mark low and worn, with a tick where a fight began.',
  ladders: 'Ladders. Each vital lights a row of segments, the way a level meter does.',
  blocks:
    'Blocks. Each vital is a bar of block characters in your game font, the way a terminal tool draws one.',
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

/** The Customize vitals fields Reset to default puts back. */
type CustomFields = keyof typeof DEFAULT_VITALS_CUSTOM;

/** Whether anything under Customize vitals differs from what Reset to
 *  default puts back, every vital on in today's order on Default, your
 *  opponent on top, Current and max, Line, and the warning and Show
 *  each hit off. */
export function customDiffers(config: Pick<UiConfig, CustomFields>): boolean {
  const d = DEFAULT_VITALS_CUSTOM;
  return (
    config.vitals_order.some((vital, i) => vital !== VITALS[i]) ||
    config.vitals_off.length > 0 ||
    Object.keys(config.vitals_colors).length > 0 ||
    config.vitals_opponent !== d.vitals_opponent ||
    config.vitals_values !== d.vitals_values ||
    config.vitals_meter !== d.vitals_meter ||
    config.vitals_warn_thirds !== d.vitals_warn_thirds ||
    config.vitals_hit !== d.vitals_hit
  );
}

/** Whether your vitals text differs from the one Text starts from,
 *  your 0.7 template `legacy` or Vosh's, which Reset to default puts
 *  back under Text. */
export function textDiffers(text: string, legacy: string | null): boolean {
  return text !== '' && text !== startVitalsText(legacy);
}

/** `order` with `vital` moved to place `to`. */
export function movedTo(order: readonly Vital[], vital: Vital, to: number): Vital[] {
  const rest = order.filter((v) => v !== vital);
  return [...rest.slice(0, to), vital, ...rest.slice(to)];
}

/** What a screen reader hears once a vital lands, like `Moves, moved
 *  to 1 of 3`. */
export function movedWords(vital: Vital, to: number, count: number): string {
  return `${VITAL_LABELS[vital]}, moved to ${to + 1} of ${count}`;
}
