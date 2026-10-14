// The calls behind the Stats pane: the last Char.Stats a session kept,
// for a window that opens after the game sent it, and the pane menu's
// pick of how the pane draws.

import { invoke } from '@tauri-apps/api/core';
import type { StatsStyle } from './uiConfig';

/** The last Char.Stats the backend kept for `session`, raw as the game
 *  sent it, or null before the first one and after the connection
 *  ends. */
export async function statsSnapshotGet(session: number): Promise<unknown> {
  return invoke('stats_snapshot_get', { session });
}

/** Save the Stats pane's style, a pick from its menu, onto the profile
 *  in front. The backend tells every window. */
export async function setStatsStyle(style: StatsStyle): Promise<void> {
  await invoke('ui_set_stats_style', { style });
}
