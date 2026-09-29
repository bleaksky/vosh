// The Aabahran GMCP packets in fixtures/gmcp/aabahran, for the tests.
// Each file holds one payload the way the server writes it, the package
// name, a space, and the JSON. The backend splits it the same way in
// vosh_gmcp::parse and emits the JSON on session://gmcp/<package>.

const FILES = import.meta.glob<string>('../../fixtures/gmcp/aabahran/*.gmcp', {
  query: '?raw',
  import: 'default',
  eager: true,
});

export interface GmcpPacket {
  package: string;
  data: unknown;
}

/** Split a GMCP payload into its package and its JSON, as the backend
 *  does. A package with no JSON carries null. */
export function splitGmcp(text: string): GmcpPacket {
  const trimmed = text.trim();
  const space = trimmed.search(/\s/);
  if (space < 0) return { package: trimmed, data: null };
  return {
    package: trimmed.slice(0, space),
    data: JSON.parse(trimmed.slice(space).trim()) as unknown,
  };
}

/** Every fixture's file name, like `char-vitals-hidden.gmcp`. */
export function aabahranFixtureNames(): string[] {
  return Object.keys(FILES)
    .map((path) => path.slice(path.lastIndexOf('/') + 1))
    .sort();
}

/** One fixture by file name, split into its package and data. */
export function aabahranPacket(name: string): GmcpPacket {
  const path = Object.keys(FILES).find((p) => p.endsWith(`/${name}`));
  if (path === undefined) throw new Error(`no GMCP fixture ${name}`);
  return splitGmcp(FILES[path]);
}
