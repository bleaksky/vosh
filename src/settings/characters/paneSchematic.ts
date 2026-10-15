import {
  isLeaf,
  type PaneKind,
  type PaneLeaf,
  type PaneNode,
  type PaneSplit,
} from '../../panel/paneLayout';
import { listJoin, possessive } from '../../lib/text';

// The small drawings of a profile's panel and sidebar in Settings >
// Characters, under Panel layout. The panel is a 180 by 110 box with a
// 1 px ring, the profile's real pane tree split by 1 px lines in each
// split's share, and the pinned Vitals strip along the bottom. The
// sidebar is a 112 by 110 box drawn the same way, with no Vitals. With
// the 12 between them the two take 304, what the card holds at the
// narrowest Settings window. Pure, so any split shape is tested without
// a page.
//
// Coordinates are SVG user units, one per CSS pixel. A line sits on the
// half pixel so it draws one crisp pixel wide, as in `M1 43.5H179`. A
// region's rect runs from the box edge (0) or from the pixel after a
// line to the next line or edge, so a label sits 10 in and 16 down from
// where the region starts, as `Map` does at (10, 16).

export const SCHEMATIC_WIDTH = 180;
export const SCHEMATIC_HEIGHT = 110;
/** The sidebar's box, narrower than the panel's as in the window. */
export const SIDEBAR_SCHEMATIC_WIDTH = 112;
/** The row the line over Vitals takes. Panes fill the rows above it. */
export const VITALS_LINE = 89;
const LABEL_X = 10;
const LABEL_Y = 16;
/** Vitals label baseline, centered on the 20 px strip under its line. */
const VITALS_LABEL_Y = 103.5;
/** A region this tall takes its label 16 down from its top. */
const LABEL_MIN_H = 22;
/** A shorter region centers its label, like the Vitals strip, and one
 *  shorter than this shows no label. */
const LABEL_SHORT_MIN_H = 12;
/** From a short region's middle to the baseline of a centered label,
 *  the same offset the Vitals label takes in its 20 px strip. */
const LABEL_CENTER_DROP = 3.5;
/** Room to keep after a label before the region's right edge. */
const LABEL_END_PAD = 6;
/** A generous average advance for the 10 px UI font, so a label that
 *  might not fit is left out rather than clipped. */
const LABEL_CHAR_W = 5.5;

export interface SchematicRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SchematicRegion {
  /** The leaf's id, which a move names. */
  id: string;
  pane: PaneKind;
  label: string;
  rect: SchematicRect;
}

export interface SchematicLabel {
  x: number;
  y: number;
  text: string;
  /** The id of the leaf it names. Vitals has none. */
  leaf?: string;
}

export interface PaneSchematic {
  width: number;
  height: number;
  /** One rect per pane, in tree order. */
  regions: SchematicRegion[];
  /** Path data for every split line and the line over Vitals. */
  lines: string;
  /** The labels that fit, the Vitals label last. */
  labels: SchematicLabel[];
  /** The drawing as one sentence, for the image's aria-label. */
  ariaLabel: string;
}

/** Where each line between `count` children falls along an extent from
 *  `start` to `end`, one per gap, in each child's share of `weights`.
 *  Every child keeps at least one pixel. */
function linePositions(weights: readonly number[], start: number, end: number): number[] {
  const count = weights.length;
  const total = weights.reduce((sum, w) => sum + (w > 0 ? w : 0), 0) || count;
  const extent = end - start;
  const out: number[] = [];
  let cumulative = 0;
  let previous = start - 1;
  for (let i = 0; i < count - 1; i += 1) {
    cumulative += weights[i] > 0 ? weights[i] : total / count;
    const want = Math.round(start + (extent * cumulative) / total);
    // One pixel of child before the line, and room for every child and
    // line still to come after it.
    const lowest = previous + 2;
    const highest = end - 2 * (count - 1 - i) - 1;
    const at = Math.max(lowest, Math.min(want, highest));
    out.push(at);
    previous = at;
  }
  return out;
}

function inner(value: number, max: number): number {
  return Math.max(1, Math.min(value, max));
}

/** The part of a box the panes fill: its width, and the last row a
 *  line between panes side by side reaches. */
interface PaneArea {
  width: number;
  bottom: number;
}

function layout(
  node: PaneNode,
  rect: SchematicRect,
  area: PaneArea,
  labelFor: (leaf: PaneLeaf) => string,
  regions: SchematicRegion[],
  lines: string[],
): void {
  if (isLeaf(node)) {
    regions.push({ id: node.id, pane: node.pane, label: labelFor(node), rect });
    return;
  }
  const kids = node.children;
  if (kids.length === 0) return;
  const weights = kids.map((k) => k.weight);
  const across = node.split === 'row';
  const start = across ? rect.x : rect.y;
  const end = start + (across ? rect.width : rect.height);
  const cuts = linePositions(weights, start, end);
  let from = start;
  kids.forEach((kid, i) => {
    const to = i < cuts.length ? cuts[i] : end;
    const child: SchematicRect = across
      ? { x: from, y: rect.y, width: to - from, height: rect.height }
      : { x: rect.x, y: from, width: rect.width, height: to - from };
    layout(kid, child, area, labelFor, regions, lines);
    if (i < cuts.length) {
      const at = cuts[i] + 0.5;
      if (across) {
        const top = inner(rect.y, area.bottom);
        const bottom = inner(rect.y + rect.height, area.bottom);
        lines.push(`M${at} ${top}V${bottom}`);
      } else {
        const left = inner(rect.x, area.width - 1);
        const right = inner(rect.x + rect.width, area.width - 1);
        lines.push(`M${left} ${at}H${right}`);
      }
      from = cuts[i] + 1;
    }
  });
}

function labelFits(region: SchematicRegion): boolean {
  const { width, height } = region.rect;
  const need = LABEL_X + region.label.length * LABEL_CHAR_W + LABEL_END_PAD;
  return height >= LABEL_SHORT_MIN_H && width >= need;
}

function labelY(rect: SchematicRect): number {
  if (rect.height >= LABEL_MIN_H) return rect.y + LABEL_Y;
  return rect.y + rect.height / 2 + LABEL_CENTER_DROP;
}

/** A node as a noun phrase: a pane's name, `Group and Chat side by
 *  side` for a split across, `Map over Affects` for a split down. */
function nounPhrase(node: PaneNode, labelFor: (leaf: PaneLeaf) => string): string {
  if (isLeaf(node)) return labelFor(node);
  const parts = node.children.map((child) => nounPhrase(child, labelFor));
  if (parts.length === 1) return parts[0];
  return node.split === 'row' ? `${listJoin(parts)} side by side` : parts.join(' over ');
}

function pronoun(node: PaneNode): string {
  return isLeaf(node) ? 'it' : 'them';
}

/** Where each of the root's children sits, as clauses, in the panel or
 *  the sidebar as `place` says. */
function rootClauses(
  root: PaneSplit,
  labelFor: (leaf: PaneLeaf) => string,
  place: 'panel' | 'sidebar',
): string[] {
  const kids = root.children;
  if (kids.length === 1) return [`${nounPhrase(kids[0], labelFor)} fills the ${place}`];
  return kids.map((kid, i) => {
    const phrase = nounPhrase(kid, labelFor);
    if (root.split === 'column') {
      return i === 0 ? `${phrase} on top` : `${phrase} below ${pronoun(kids[i - 1])}`;
    }
    if (i === 0) return `${phrase} on the left`;
    if (i === kids.length - 1) return `${phrase} on the right`;
    return kids.length === 3
      ? `${phrase} in the middle`
      : `${phrase} next to ${pronoun(kids[i - 1])}`;
  });
}

/** The drawing as a sentence, like `Ilsabet's panel. Map on top,
 *  Affects below it, Vitals along the bottom.` */
export function schematicSentence(
  root: PaneSplit,
  labelFor: (leaf: PaneLeaf) => string,
  owner: string,
): string {
  const head = `${possessive(owner)} panel.`;
  if (root.children.length === 0) return `${head} No panes, only Vitals along the bottom.`;
  return `${head} ${rootClauses(root, labelFor, 'panel').join(', ')}, Vitals along the bottom.`;
}

/** The sidebar's drawing as a sentence, like `Ilsabet's sidebar. Stats
 *  on top, Group below it.` */
export function sidebarSentence(
  tree: PaneSplit,
  labelFor: (leaf: PaneLeaf) => string,
  owner: string,
): string {
  const head = `${possessive(owner)} sidebar`;
  if (tree.children.length === 0) return `${head} holds no panes.`;
  return `${head}. ${rootClauses(tree, labelFor, 'sidebar').join(', ')}.`;
}

/** The labels of the regions that fit. */
function fittedLabels(regions: SchematicRegion[]): SchematicLabel[] {
  return regions.filter(labelFits).map((region) => ({
    x: region.rect.x + LABEL_X,
    y: labelY(region.rect),
    text: region.label,
    leaf: region.id,
  }));
}

/** Draw `root`, the tree a profile's panel shows, for `owner` (the
 *  name the sentence uses). `labelFor` names each pane. */
export function paneSchematic(
  root: PaneSplit,
  labelFor: (leaf: PaneLeaf) => string,
  owner: string,
): PaneSchematic {
  const regions: SchematicRegion[] = [];
  const lines: string[] = [];
  layout(
    root,
    { x: 0, y: 0, width: SCHEMATIC_WIDTH, height: VITALS_LINE },
    { width: SCHEMATIC_WIDTH, bottom: VITALS_LINE },
    labelFor,
    regions,
    lines,
  );
  lines.push(`M1 ${VITALS_LINE + 0.5}H${SCHEMATIC_WIDTH - 1}`);
  const labels = fittedLabels(regions);
  labels.push({ x: LABEL_X, y: VITALS_LABEL_Y, text: 'Vitals' });
  return {
    width: SCHEMATIC_WIDTH,
    height: SCHEMATIC_HEIGHT,
    regions,
    lines: lines.join(''),
    labels,
    ariaLabel: schematicSentence(root, labelFor, owner),
  };
}

/** Draw `tree`, the profile's sidebar, for `owner`. Its panes fill the
 *  whole box, and an empty sidebar draws no line and no label. */
export function sidebarSchematic(
  tree: PaneSplit,
  labelFor: (leaf: PaneLeaf) => string,
  owner: string,
): PaneSchematic {
  const regions: SchematicRegion[] = [];
  const lines: string[] = [];
  layout(
    tree,
    { x: 0, y: 0, width: SIDEBAR_SCHEMATIC_WIDTH, height: SCHEMATIC_HEIGHT },
    { width: SIDEBAR_SCHEMATIC_WIDTH, bottom: SCHEMATIC_HEIGHT - 1 },
    labelFor,
    regions,
    lines,
  );
  return {
    width: SIDEBAR_SCHEMATIC_WIDTH,
    height: SCHEMATIC_HEIGHT,
    regions,
    lines: lines.join(''),
    labels: fittedLabels(regions),
    ariaLabel: sidebarSentence(tree, labelFor, owner),
  };
}
