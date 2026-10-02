import { profileDisplayName } from './characterProfiles';

// How Settings names a profile in its copy. The display name comes
// from characterProfiles.ts, the one Characters uses, so every page
// reads the reserved profile the same way.

export { profileDisplayName };

/** The possessive of a profile's display name, like `Ilsabet's`. */
export function profilePossessive(name: string): string {
  return `${profileDisplayName(name)}'s`;
}
