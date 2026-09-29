// How Settings names a profile in its copy.

/** The reserved `default` profile reads Default. Every other profile
 *  reads as its name. */
export function profileDisplayName(name: string): string {
  return name === 'default' ? 'Default' : name;
}

/** The possessive of a profile's display name, like `Erelei's`. */
export function profilePossessive(name: string): string {
  return `${profileDisplayName(name)}'s`;
}
