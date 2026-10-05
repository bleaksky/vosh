// The tick timer and its settings.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/** The tick timer as the session loop reports it on session://tick,
 *  four times a second and on every tick. */
export interface TickPayload {
  enabled: boolean;
  interval_ms: number;
  /** Time left until the expected tick, 0 once it has passed. */
  remaining_ms: number;
  /** Time since the last tick. It keeps growing past the interval
   *  while the game runs late. */
  elapsed_ms: number;
  /** The expected tick has come and the game's tick has not. */
  overdue: boolean;
  /** The game's own tick decides when the timer fires. */
  synced: boolean;
  /** This report is the tick itself, so the sound plays once. */
  fired: boolean;
  sound: boolean;
}

export async function onTick(cb: (payload: TickPayload) => void): Promise<UnlistenFn> {
  return listen<TickPayload>('session://tick', (event) => {
    cb(event.payload);
  });
}

// Live tick-timer configuration. Mirrors TickConfig in tick.rs on the
// backend. Optional fields use null to mean "feature off / use
// default"; the backend trims empty strings to null on write.
export interface TickConfig {
  enabled: boolean;
  interval_secs: number;
  auto_fire: string | null;
  sound: boolean;
  reset_pattern: string | null;
  warn_at_secs: number | null;
  warn_message: string | null;
  warn_color: string | null;
}

export async function tickGetConfig(): Promise<TickConfig> {
  return invoke('tick_get_config');
}

export async function tickSetConfig(config: TickConfig): Promise<TickConfig> {
  return invoke('tick_set_config', { config });
}

export async function subscribeTickConfigChanged(
  cb: (cfg: TickConfig) => void,
): Promise<UnlistenFn> {
  return listen<TickConfig>('vosh://tick-config-changed', (event) => cb(event.payload));
}
