// The worlds Vosh knows by name, and the name each host shows as.

/** A world known by name, and where you connect to play it. */
export interface KnownWorld {
  /** A host matches this domain or any subdomain of it. */
  domain: string;
  name: string;
  host: string;
  port: number;
}

/** Worlds known by name. Any other host shows as typed. Mirrors
 *  KNOWN_WORLDS in src-tauri/src/profile/worlds.rs, where a test reads this
 *  list. */
export const KNOWN_WORLDS: readonly KnownWorld[] = [
  {
    domain: 'theforsakenlands.com',
    name: 'The Forsaken Lands',
    host: 'play.theforsakenlands.com',
    port: 1848,
  },
];

/** A host as Vosh compares it, trimmed, in lower case and without a
 *  closing dot. Mirrors host_key in src-tauri/src/profile/worlds.rs. */
export function hostKey(host: string): string {
  return host.trim().toLowerCase().replace(/\.$/, '');
}

/** The known world a host plays, matching its domain or any subdomain,
 *  or undefined for any other host. */
export function knownWorld(host: string): KnownWorld | undefined {
  const clean = hostKey(host);
  return KNOWN_WORLDS.find((w) => clean === w.domain || clean.endsWith(`.${w.domain}`));
}

/** The display name for a host, like `The Forsaken Lands` for
 *  `play.theforsakenlands.com`. Unknown hosts show as typed. */
export function worldName(host: string): string {
  return knownWorld(host)?.name ?? host.trim();
}

/** The world a host and port play, like `The Forsaken Lands` on its own
 *  port 1848 and `The Forsaken Lands 1825` on the build port. A known
 *  world adds a port that is not its own, so two ports of one game read
 *  apart. Any other host has no port of its own and shows as typed.
 *  Mirrors world_label in src-tauri/src/profile/worlds.rs. */
export function worldLabel(host: string, port: number): string {
  const world = knownWorld(host);
  if (!world) return host.trim();
  return port === world.port ? world.name : `${world.name} ${port}`;
}
