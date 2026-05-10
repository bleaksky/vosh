// Sector palette ported from the Forsaken Lands web map (web/static/map.js).
// Each sector has three colors: a dim fill, a mid border, and a bright halo
// used for area-name watermarks. The mapping and server map views both pull
// from this table so a tile in either mode reads the same.

export interface SectorTheme {
  name: string;
  fill: string;
  border: string;
  halo: string;
}

export const SECTORS: Record<number, SectorTheme> = {
  0: { name: 'Inside', fill: '#222228', border: '#5a5a64', halo: '#8888a0' },
  1: { name: 'City', fill: '#28221a', border: '#a08a5a', halo: '#c4a872' },
  2: { name: 'Field', fill: '#182418', border: '#4a8a4a', halo: '#5faf5f' },
  3: { name: 'Forest', fill: '#102010', border: '#2a7a2a', halo: '#2aaa2a' },
  4: { name: 'Hills', fill: '#242418', border: '#8a8a4a', halo: '#afaf5f' },
  5: { name: 'Mountain', fill: '#1c1c24', border: '#5a5a6a', halo: '#7a7a8a' },
  6: { name: 'Water', fill: '#101c2c', border: '#3a6a9a', halo: '#4a9adf' },
  7: { name: 'Deep Water', fill: '#0c1434', border: '#2a5a8a', halo: '#2a7adf' },
  8: { name: 'Swamp', fill: '#1a1c10', border: '#4a4a2a', halo: '#6a6a2a' },
  9: { name: 'Air', fill: '#142028', border: '#5a8aaa', halo: '#7abada' },
  10: { name: 'Desert', fill: '#2a2210', border: '#aa8a3a', halo: '#ddb030' },
  11: { name: 'Lava', fill: '#2a1010', border: '#aa3a2a', halo: '#df4a2a' },
  12: { name: 'Snow', fill: '#222428', border: '#9a9aa0', halo: '#c0c0c8' },
};

export const MAP_COLORS = {
  bg: '#07090f',
  /// Player's room cell uses a sector-style fill+border pair: a dim
  /// pink interior with a bright pink outline so it reads the same
  /// way as a regular sector tile, just in pink.
  origin: '#ff3399',
  originFill: '#3a1424',
  dest: '#c83030',
  destGlow: 'rgba(200,48,48,0.15)',
  corridor: 'rgba(140,145,160,0.45)',
  xarea: 'rgba(120,120,130,0.20)',
  text: '#888',
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
