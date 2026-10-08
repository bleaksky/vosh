// Which drawing the map uses, and how the choice is stored. Pure over
// a storage reader so the fallbacks are unit tested.

/** How the map draws. `squares` is the default, as it was before the
 *  window redesign: your room held at the center, doors in their
 *  state colors, the floors above and below, and the terrain. `3d`
 *  stacks the floors as boxes you can turn and tilt. The redesign drew
 *  a `plain` style for a while. That drawing is gone, and a stored one
 *  is ignored. */
export type MapStyle = 'squares' | 'glyphs' | 'tileset' | '3d';

/** The styles the map's menu offers, in its order. */
export const MAP_STYLE_CHOICES: readonly MapStyle[] = ['squares', 'glyphs', 'tileset', '3d'];

export const MAP_STYLE_KEY = 'vosh.map.style';
// The key before the redesign. It held `squares` by default, written
// on every mount, so only `glyphs` or `tileset` there is a choice.
const LEGACY_STYLE_KEY = 'vosh.layout.serverMapStyle';

/** The style to draw with. The map writes its style on every mount, so
 *  a stored `plain` is the redesign's old default and not a choice you
 *  made. It gives way to a pick under the earlier key, then squares. */
export function loadMapStyle(storage: Pick<Storage, 'getItem'>): MapStyle {
  try {
    const value = storage.getItem(MAP_STYLE_KEY);
    if (MAP_STYLE_CHOICES.includes(value as MapStyle)) return value as MapStyle;
    const legacy = storage.getItem(LEGACY_STYLE_KEY);
    if (legacy === 'glyphs' || legacy === 'tileset') return legacy;
  } catch {
    // Storage can refuse to read. The default below stands.
  }
  return 'squares';
}
