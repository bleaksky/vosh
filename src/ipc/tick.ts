// The tick timer and its settings.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { DAYLIGHT_CHANGED, TICK, TICK_CONFIG_CHANGED } from './events';
import { sessionOf } from './session';

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

/** Hear each report of a session's tick timer, with that session. */
export async function onTick(
  cb: (payload: TickPayload, session: number) => void,
): Promise<UnlistenFn> {
  return listen<TickPayload & { session?: number }>(TICK, (event) => {
    cb(event.payload, sessionOf(event.payload));
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

/** A profile's tick settings, the selected session's profile's when it
 *  names none. */
export async function tickGetConfig(profile?: string | null): Promise<TickConfig> {
  return invoke('tick_get_config', { profile });
}

/** Save a profile's tick settings, which every count on it follows. */
export async function tickSetConfig(
  config: TickConfig,
  profile?: string | null,
): Promise<TickConfig> {
  return invoke('tick_set_config', { config, profile });
}

export async function subscribeTickConfigChanged(
  cb: (cfg: TickConfig) => void,
): Promise<UnlistenFn> {
  return listen<TickConfig>(TICK_CONFIG_CHANGED, (event) => cb(event.payload));
}

/** Whether the sun is up in a session's game, as its latest World.Time
 *  said, `Daylight` in src-tauri/src/tick.rs. */
export type Daylight = 'day' | 'night';

function asDaylight(value: unknown): Daylight | null {
  return value === 'day' || value === 'night' ? value : null;
}

/** The day or night in the game of `session`, or the selected one, as
 *  its latest World.Time said, through a drop too. Null before the
 *  first. */
export async function daylightGet(session?: number): Promise<Daylight | null> {
  return asDaylight(await invoke<unknown>('daylight_get', { session }));
}

/** Hear the game of a session turn to day or night, with that session. */
export async function subscribeDaylightChanged(
  cb: (phase: Daylight, session: number) => void,
): Promise<UnlistenFn> {
  return listen<{ phase?: unknown; session?: number }>(DAYLIGHT_CHANGED, (event) => {
    const phase = asDaylight(event.payload.phase);
    if (phase) cb(phase, sessionOf(event.payload));
  });
}
