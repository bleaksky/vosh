// The Aabahran GMCP packets in fixtures/gmcp/aabahran, for the tests.
// Each file holds one payload the way the server writes it, the package
// name, a space, and the JSON. The backend splits it the same way in
// vosh_protocol::gmcp::parse and sends the JSON as the data of
// {session, data} on session://gmcp/ and the package name, its dots
// turned to dashes.

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

// The Comm.Channel packets sit in their own folder, since only the chat
// store reads them and the prompt engine keeps no view of chat.
const CHAT_FILES = import.meta.glob<string>('../../fixtures/gmcp/aabahran/chat/*.gmcp', {
  query: '?raw',
  import: 'default',
  eager: true,
});

/** Every chat fixture's file name, like `tell.gmcp`. */
export function aabahranChatFixtureNames(): string[] {
  return Object.keys(CHAT_FILES)
    .map((path) => path.slice(path.lastIndexOf('/') + 1))
    .sort();
}

/** One chat fixture by file name, split into its package and data. */
export function aabahranChatPacket(name: string): GmcpPacket {
  const path = Object.keys(CHAT_FILES).find((p) => p.endsWith(`/${name}`));
  if (path === undefined) throw new Error(`no chat fixture ${name}`);
  return splitGmcp(CHAT_FILES[path]);
}

// The Map.Tiles packets sit in their own folder too, since only the map
// view reads them.
const MAP_FILES = import.meta.glob<string>('../../fixtures/gmcp/aabahran/map/*.gmcp', {
  query: '?raw',
  import: 'default',
  eager: true,
});

/** Every Map.Tiles fixture's file name. */
export function aabahranMapFixtureNames(): string[] {
  return Object.keys(MAP_FILES)
    .map((path) => path.slice(path.lastIndexOf('/') + 1))
    .sort();
}

/** One Map.Tiles fixture by file name, split into its package and data. */
export function aabahranMapPacket(name: string): GmcpPacket {
  const path = Object.keys(MAP_FILES).find((p) => p.endsWith(`/${name}`));
  if (path === undefined) throw new Error(`no map fixture ${name}`);
  return splitGmcp(MAP_FILES[path]);
}
