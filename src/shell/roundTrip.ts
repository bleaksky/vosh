// How the status line shows the round trip to the game. Under
// 300 ms you cannot feel the network on Aabahran's
// 250 ms pulse, so the reading sits in tertiary. From 300 ms your
// commands land a pulse late and it takes the warn tone. From a second
// you are stalling, and it reads in seconds to one decimal in the
// danger text tone. The text color alone changes, so the tick keeps the
// only ground on the line. `#lag` writes a reading the same way.

/** From here a reading is slow and never gives way. */
export const SLOW_MS = 300;
/** From here a reading is a stall and reads in seconds. */
export const STALL_MS = 1000;

/** The widest reading the line measures, so one that loses a digit
 *  moves nothing. */
export const WIDEST_ROUND_TRIP = '999ms';

export type RoundTripTone = 'fine' | 'warn' | 'danger';

/** `38ms` under a second, then `1.4s`. */
export function roundTripText(ms: number): string {
  return ms < STALL_MS ? `${Math.round(ms)}ms` : `${(ms / 1000).toFixed(1)}s`;
}

export function roundTripTone(ms: number): RoundTripTone {
  return ms >= STALL_MS ? 'danger' : ms >= SLOW_MS ? 'warn' : 'fine';
}
