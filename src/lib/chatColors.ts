// Per-channel pastel colors for the chat pane, from the Ember palette.
// Known channel names get a fixed color so tells are always the same
// hue across sessions. Unknown pane names (custom trigger routes, MUD
// channels not listed here) hash onto the same pastel set, so every
// channel still gets a stable color of its own.

const PASTELS = [
  '#cba6dd', // magenta
  '#8fdaa8', // jade
  '#97dde8', // cyan
  '#ecc985', // gold
  '#9bbdf0', // blue
  '#ea8f80', // coral
  '#b8e0a0', // moss
  '#e8b8d0', // rose
];

// A Map rather than an object literal: pane names come straight from
// server data and user-defined routes, and keys like "constructor"
// must not walk the prototype chain.
const FIXED = new Map<string, string>([
  ['tell', '#cba6dd'],
  ['tells', '#cba6dd'],
  ['group', '#b48ec9'],
  ['gtell', '#b48ec9'],
  ['say', '#97dde8'],
  ['says', '#97dde8'],
  ['gossip', '#8fdaa8'],
  ['chat', '#8fdaa8'],
  ['auction', '#ecc985'],
  ['shout', '#ea8f80'],
  ['yell', '#ea8f80'],
  ['ooc', '#9bbdf0'],
  ['quote', '#9bbdf0'],
  ['clan', '#b8e0a0'],
  ['cabal', '#b8e0a0'],
  ['pray', '#e8b8d0'],
]);

export function chatChannelColor(pane: string): string {
  const key = pane.trim().toLowerCase();
  const fixed = FIXED.get(key);
  if (fixed) return fixed;
  // FNV-1a over the pane name; stable across sessions and windows.
  let hash = 0x811c9dc5;
  for (let i = 0; i < key.length; i++) {
    hash ^= key.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return PASTELS[(hash >>> 0) % PASTELS.length];
}
