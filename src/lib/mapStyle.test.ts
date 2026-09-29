import { describe, expect, it } from 'vitest';
import { MAP_STYLE_CHOICES, MAP_STYLE_KEY, loadMapStyle } from './mapStyle';

const LEGACY = 'vosh.layout.serverMapStyle';

function store(values: Record<string, string>) {
  return { getItem: (key: string) => values[key] ?? null };
}

describe('loadMapStyle', () => {
  it('draws squares when nothing is stored', () => {
    expect(loadMapStyle(store({}))).toBe('squares');
  });

  it('keeps a style you picked', () => {
    expect(loadMapStyle(store({ [MAP_STYLE_KEY]: 'squares' }))).toBe('squares');
    expect(loadMapStyle(store({ [MAP_STYLE_KEY]: 'glyphs' }))).toBe('glyphs');
    expect(loadMapStyle(store({ [MAP_STYLE_KEY]: 'tileset' }))).toBe('tileset');
  });

  it('sets aside the plain drawing the redesign stored by default', () => {
    expect(loadMapStyle(store({ [MAP_STYLE_KEY]: 'plain' }))).toBe('squares');
    expect(loadMapStyle(store({ [MAP_STYLE_KEY]: 'plain', [LEGACY]: 'glyphs' }))).toBe('glyphs');
  });

  it('honors a glyphs or tileset pick from before the redesign', () => {
    expect(loadMapStyle(store({ [LEGACY]: 'tileset' }))).toBe('tileset');
    expect(loadMapStyle(store({ [LEGACY]: 'squares' }))).toBe('squares');
    expect(loadMapStyle(store({ [MAP_STYLE_KEY]: 'squares', [LEGACY]: 'glyphs' }))).toBe('squares');
  });

  it('ignores values it does not know and storage that throws', () => {
    expect(loadMapStyle(store({ [MAP_STYLE_KEY]: 'hexes' }))).toBe('squares');
    const broken = {
      getItem: () => {
        throw new Error('denied');
      },
    };
    expect(loadMapStyle(broken)).toBe('squares');
  });
});

describe('MAP_STYLE_CHOICES', () => {
  it('offers the styles from before the redesign, squares first', () => {
    expect(MAP_STYLE_CHOICES).toEqual(['squares', 'glyphs', 'tileset']);
  });
});
