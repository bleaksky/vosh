// The round trip to the game, which each session reads every two
// seconds (Round Trip Readout).

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ROUND_TRIP } from './events';
import { sessionOf } from './session';

/** The round trip as the session loop reports it on session://round-trip:
 *  whole milliseconds, or null once the connection ends. */
export interface RoundTripPayload {
  ms: number | null;
}

/** Hear each reading of a session's round trip, with that session. */
export async function onRoundTrip(
  cb: (payload: RoundTripPayload, session: number) => void,
): Promise<UnlistenFn> {
  return listen<RoundTripPayload & { session?: number }>(ROUND_TRIP, (event) => {
    cb({ ms: event.payload.ms }, sessionOf(event.payload));
  });
}
