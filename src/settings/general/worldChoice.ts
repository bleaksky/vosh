import type { ConnectionTarget } from '../../ipc/session';
import { KNOWN_WORLDS, knownWorld, worldLabel } from '../../lib/knownWorlds';
import type { SelectOption } from '../../ui';

// What the World select in General › Connection offers for the target of
// the selected session. Each known world dials its own port. A target
// that is not a known world on its own port shows as the saved choice,
// the way its row reads it, so the build port reads The Forsaken Lands
// 1825 and picking The Forsaken Lands sets 1848. Other… clears host and
// port for you to type.

/** Other…, which clears host and port. */
export const OTHER = 'other';
/** The target as it is, when no known world on its own port matches. */
export const SAVED = 'saved';

/** The choice that picks `world`. */
export function worldValue(world: { domain: string }): string {
  return `world:${world.domain}`;
}

/** The choices for `target`, and the one it shows as. */
export function worldChoice(target: ConnectionTarget): { options: SelectOption[]; value: string } {
  const world = knownWorld(target.host);
  const own = world !== undefined && target.port === world.port;
  const options = KNOWN_WORLDS.map((w) => ({ value: worldValue(w), label: w.name }));
  if (!own) options.push({ value: SAVED, label: worldLabel(target.host, target.port) });
  options.push({ value: OTHER, label: 'Other…' });
  return { options, value: own ? worldValue(world) : SAVED };
}
