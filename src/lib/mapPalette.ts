// Sector palette ported from the Forsaken Lands web map (web/static/map.js).
// Each sector has three colors: a dim fill, a mid border, and a bright halo
// used for area-name watermarks. The mapping and server map views both pull
// from this table so a tile in either mode reads the same.

export interface SectorTheme {
  name: string;
  fill: string;
  border: string;
  halo: string;
  /// Single character drawn in the glyph map for cells of this
  /// sector. Picked to disambiguate from neighbors: hills `^` vs
  /// mountain `M`, water `~` vs deep water `≈`, etc. Stay ASCII or
  /// well-supported Unicode so any monospace font renders the cell
  /// at exactly 1ch.
  glyph: string;
}

export const SECTORS: Record<number, SectorTheme> = {
  0: { name: 'Inside', fill: '#222228', border: '#5a5a64', halo: '#8888a0', glyph: '#' },
  1: { name: 'City', fill: '#28221a', border: '#a08a5a', halo: '#c4a872', glyph: '+' },
  2: { name: 'Field', fill: '#182418', border: '#4a8a4a', halo: '#5faf5f', glyph: '.' },
  3: { name: 'Forest', fill: '#102010', border: '#2a7a2a', halo: '#2aaa2a', glyph: '*' },
  4: { name: 'Hills', fill: '#242418', border: '#8a8a4a', halo: '#afaf5f', glyph: '^' },
  5: { name: 'Mountain', fill: '#1c1c24', border: '#5a5a6a', halo: '#7a7a8a', glyph: 'M' },
  6: { name: 'Water', fill: '#101c2c', border: '#3a6a9a', halo: '#4a9adf', glyph: '~' },
  7: { name: 'Deep Water', fill: '#0c1434', border: '#2a5a8a', halo: '#2a7adf', glyph: '≈' },
  8: { name: 'Swamp', fill: '#1a1c10', border: '#4a4a2a', halo: '#6a6a2a', glyph: ',' },
  9: { name: 'Air', fill: '#142028', border: '#5a8aaa', halo: '#7abada', glyph: "'" },
  10: { name: 'Desert', fill: '#2a2210', border: '#aa8a3a', halo: '#ddb030', glyph: ':' },
  11: { name: 'Lava', fill: '#2a1010', border: '#aa3a2a', halo: '#df4a2a', glyph: '!' },
  12: { name: 'Snow', fill: '#222428', border: '#9a9aa0', halo: '#c0c0c8', glyph: 'o' },
};

/// Fallback for sector codes the server sends that we have not yet
/// mapped (e.g. a new terrain type on Aabahran). Reads as "something
/// here" without screaming the way `?` did — but still slightly
/// noticeable so we can find missing codes during play.
export const UNKNOWN_GLYPH = '·';

/// Color to render a sector's glyph at, given a dim level from
/// `dimLevel()` (0 = nearest/full, 1 = mid, 2 = far/faint). The halo
/// color is the source; dim levels mix toward the surface bg via
/// rgba alpha. Same source-of-truth as the squares/tileset modes so
/// a tile of the same sector reads the same color across modes.
export function sectorGlyphColor(code: string | undefined, dimLevel: number): string {
  const theme = sectorForCode(code);
  const alpha = dimLevel <= 0 ? 1 : dimLevel === 1 ? 0.65 : 0.38;
  return hexToRgba(theme.halo, alpha);
}

// Theme-dependent slots (bg, origin, originFill, text) are exposed as
// getters that read from --c-* custom properties at access time so the
// canvas tracks the active app theme. Terrain-meaningful slots stay
// fixed regardless of theme. Fallbacks match the Kanso Zen palette
// for the first paint before applyTheme has installed CSS vars.

function readCssVar(name: string, fallback: string): string {
  if (typeof document === 'undefined') return fallback;
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

export const MAP_COLORS = {
  get bg(): string {
    return readCssVar('--c-surface', '#090e13');
  },
  /// Player's room cell uses a sector-style fill+border pair: a dim
  /// tint of the accent inside with the bright accent as the outline,
  /// so the player tile reads the same shape as a regular sector tile,
  /// just in the user's chosen accent color.
  get origin(): string {
    return readCssVar('--c-accent', '#ff3399');
  },
  get originFill(): string {
    return readCssVar('--c-accent-soft', 'rgba(255, 51, 153, 0.09)');
  },
  get text(): string {
    return readCssVar('--c-text-faint', '#6e7681');
  },
  dest: '#c83030',
  destGlow: 'rgba(200,48,48,0.15)',
  corridor: 'rgba(140,145,160,0.45)',
  xarea: 'rgba(120,120,130,0.20)',
  pathLine: 'rgba(196,168,114,0.7)',
};

// Aabahran's GMCP Map.Tiles sector codes are characters: 0..9, a, b, c.
// Map them to our sector index.
const SERVER_CODE_TO_SECTOR: Record<string, number> = {
  '0': 0, // Inside
  '1': 1, // City
  '2': 2, // Field
  '3': 3, // Forest
  '4': 4, // Hills
  '5': 5, // Mountain
  '6': 6, // Water
  '7': 7, // Deep Water
  '8': 8, // Swamp
  '9': 9, // Air
  a: 10, // Desert
  b: 11, // Lava
  c: 12, // Snow
};

export function sectorForCode(code: string | undefined): SectorTheme {
  if (!code) return SECTORS[0];
  const idx = SERVER_CODE_TO_SECTOR[code];
  return SECTORS[idx ?? 0];
}

// Best-effort mapping from a Room.Info terrain string to a sector index.
// Names roughly match what ROM 2.4 derivatives report.
export function sectorForTerrain(terrain: string | undefined): SectorTheme {
  if (!terrain) return SECTORS[0];
  const t = terrain.toLowerCase();
  if (t.includes('inside') || t.includes('road') || t.includes('indoor')) return SECTORS[0];
  if (t.includes('city') || t.includes('street')) return SECTORS[1];
  if (t.includes('field') || t.includes('grass') || t.includes('pasture')) return SECTORS[2];
  if (t.includes('forest') || t.includes('wood')) return SECTORS[3];
  if (t.includes('hill')) return SECTORS[4];
  if (t.includes('mountain')) return SECTORS[5];
  if (t.includes('underwater')) return SECTORS[7];
  if (t.includes('water')) return t.includes('noswim') ? SECTORS[7] : SECTORS[6];
  if (t.includes('swamp') || t.includes('marsh') || t.includes('bog')) return SECTORS[8];
  if (t.includes('air')) return SECTORS[9];
  if (t.includes('desert') || t.includes('sand')) return SECTORS[10];
  if (t.includes('lava') || t.includes('volcano') || t.includes('inferno')) return SECTORS[11];
  if (t.includes('ice') || t.includes('arctic') || t.includes('tundra') || t.includes('snow')) {
    return SECTORS[12];
  }
  return SECTORS[0];
}

// Convert a hex string like "#aabbcc" to an rgba() string at the given alpha.
export function hexToRgba(hex: string, alpha: number): string {
  const h = hex.replace('#', '');
  if (h.length !== 6) return hex;
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}
