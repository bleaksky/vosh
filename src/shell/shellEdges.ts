import type { PanelSide } from '../ipc/uiConfig';

// Which way the frame's two edges drag. AppShell steps and drags them
// through this, so either panel side widens a column toward the middle.

/** The panel's edge, or the sessions sidebar's line. */
export type Edge = 'panel' | 'sessions';

/** How much a move of `dx` across the screen widens the column `edge`
 *  sizes. An edge on the right of its column widens it moving left, and
 *  one on the left widens it moving right. */
export function edgeGrowth(edge: Edge, panelSide: PanelSide, dx: number): number {
  const onRight = (edge === 'panel') === (panelSide === 'right');
  return onRight ? -dx : dx;
}
