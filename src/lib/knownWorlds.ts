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

/** The known world a host plays, matching its domain or any subdomain,
 *  or undefined for any other host. */
export function knownWorld(host: string): KnownWorld | undefined {
  const clean = host.trim().toLowerCase().replace(/\.$/, '');
  return KNOWN_WORLDS.find((w) => clean === w.domain || clean.endsWith(`.${w.domain}`));
}

/** The display name for a host, like `The Forsaken Lands` for
 *  `play.theforsakenlands.com`. Unknown hosts show as typed. */
export function worldName(host: string): string {
  return knownWorld(host)?.name ?? host.trim();
}
