// The parts of an alert, as the Alert row of a trigger and the card of
// an alert preset edit them. Rust reads the same table, so the page reads
// it the way `AlertParts` in crates/automation/src/alert.rs and the
// trigger reader in crates/automation/src/trigger/store.rs do.

import type { AlertParts } from '../ipc/automation';

/** The three parts the Alert row presses on and off, by the key each
 *  one sets. */
export type AlertPart = 'banner' | 'sound' | 'attention';

/** The tone Sound plays until you pick another, Chime, the first of the
 *  tones. Its row under Advanced shows it while Sound is off. */
export const FIRST_TONE = 'chime';

/** How long Bounce asks for you until you pick another, once. Its row
 *  under Advanced shows it while Bounce is off. */
export const FIRST_ATTENTION = 'once';

/** The table of an alert with nothing on, as Rust reads an empty one.
 *  It rings only while you are not looking at its session, and its
 *  banner shows the title alone. */
const QUIET: AlertParts = { banner: false, background: true, words: false };

const isSwitch = (value: unknown) => value === undefined || typeof value === 'boolean';

/** An alert table as Rust reads it, or undefined where Rust reads none.
 *  A missing key takes its default, background on and every other part
 *  off, and an attention Rust does not know reads as none. A banner,
 *  background or words that is no switch, or a sound that is no text,
 *  fails the table in Rust, so the trigger keeps its actions and rings
 *  nothing. */
export function normalizeAlert(raw: unknown): AlertParts | undefined {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return undefined;
  const r = raw as Record<string, unknown>;
  if (!isSwitch(r.banner) || !isSwitch(r.background) || !isSwitch(r.words)) return undefined;
  if (r.sound !== undefined && r.sound !== null && typeof r.sound !== 'string') return undefined;
  const out: AlertParts = {
    banner: r.banner === true,
    background: r.background !== false,
    words: r.words === true,
  };
  if (typeof r.sound === 'string') out.sound = r.sound;
  if (r.attention === 'once' || r.attention === 'until') out.attention = r.attention;
  return out;
}

/** `alert` with one part pressed on or off. A trigger that rings none
 *  starts from the default table. Sound comes on with Chime and Bounce
 *  with once, and a part that is already on keeps its tone or its
 *  length. */
export function withAlertPart(
  alert: AlertParts | undefined,
  part: AlertPart,
  on: boolean,
): AlertParts {
  const next: AlertParts = { ...(alert ?? QUIET) };
  if (part === 'banner') next.banner = on;
  else if (!on) delete next[part];
  else if (part === 'sound') next.sound = next.sound ?? FIRST_TONE;
  else next.attention = next.attention ?? FIRST_ATTENTION;
  return next;
}

/** `alert` with `patch` laid over it, from the default table when the
 *  trigger rings none. The table holds no tone for a Sound that is off,
 *  so a tone picked there turns Sound on, and a length turns Bounce on
 *  the same way. */
export function withAlertParts(
  alert: AlertParts | undefined,
  patch: Partial<AlertParts>,
): AlertParts {
  return { ...(alert ?? QUIET), ...patch };
}

/** `alert`, or undefined when it is the default table, with nothing on,
 *  background on and words off. A trigger you never gave an alert, or
 *  whose parts you all released, then saves no `alert` table. */
export function alertOrNone(alert: AlertParts | undefined): AlertParts | undefined {
  if (!alert) return undefined;
  const quiet = !alert.banner && alert.sound === undefined && alert.attention === undefined;
  return quiet && alert.background && !alert.words ? undefined : alert;
}
