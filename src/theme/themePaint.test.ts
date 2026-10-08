import { describe, expect, it } from 'vitest';
import {
  THEME_PAINT_KEY,
  bootPaintPhase,
  parseThemePaint,
  pickPaintSide,
  prepaintTheme,
  readThemePaint,
  writeThemePaint,
  type PaintRoot,
  type PaintStorage,
  type ThemePaint,
  type ThemePaintSide,
} from './themePaint';

const light: ThemePaintSide = {
  id: 'rubric',
  appearance: 'light',
  vars: { '--bg': '#f4efe4', '--text': '#2b2620', '--xterm-bg': '#f4efe4' },
};
const dark: ThemePaintSide = {
  id: 'tokyo-night',
  appearance: 'dark',
  vars: { '--bg': '#1a1b26', '--text': '#c0caf5', '--xterm-bg': '#1a1b26' },
};

const following: ThemePaint = { v: 1, follow: true, light, dark };
const manual: ThemePaint = { v: 1, follow: false, manual: light };
/** By night after a drop, Tokyo Night for the night and Rubric for the
 *  day. */
const game: ThemePaint = { v: 1, follow: 'game', day: light, night: dark, phase: 'night' };

function memoryStorage(initial: Record<string, string> = {}): PaintStorage & {
  data: Record<string, string>;
} {
  const data = { ...initial };
  return {
    data,
    getItem: (key) => (key in data ? data[key] : null),
    setItem: (key, value) => {
      data[key] = value;
    },
  };
}

function fakeRoot(): PaintRoot & { attrs: Record<string, string>; vars: Record<string, string> } {
  const attrs: Record<string, string> = {};
  const vars: Record<string, string> = {};
  return {
    attrs,
    vars,
    setAttribute: (name, value) => {
      attrs[name] = value;
    },
    style: {
      setProperty: (name, value) => {
        vars[name] = value;
      },
    },
  };
}

describe('writeThemePaint and readThemePaint', () => {
  it('round trips a paint through storage', () => {
    const storage = memoryStorage();
    expect(writeThemePaint(following, storage)).toBe(true);
    expect(typeof storage.data[THEME_PAINT_KEY]).toBe('string');
    expect(readThemePaint(storage)).toEqual(following);

    writeThemePaint(manual, storage);
    expect(readThemePaint(storage)).toEqual(manual);
  });

  it('leaves storage alone when it holds the paint already', () => {
    const storage = memoryStorage();
    let writes = 0;
    const counting: PaintStorage = {
      getItem: storage.getItem,
      setItem: (key, value) => {
        writes += 1;
        storage.setItem(key, value);
      },
    };
    writeThemePaint(following, counting);
    writeThemePaint(following, counting);
    expect(writes).toBe(1);
    writeThemePaint(manual, counting);
    expect(writes).toBe(2);
  });

  it('reads nothing from empty storage or no storage', () => {
    expect(readThemePaint(memoryStorage())).toBeNull();
    expect(readThemePaint(null)).toBeNull();
    expect(writeThemePaint(manual, null)).toBe(false);
  });

  it('survives storage that throws', () => {
    const broken: PaintStorage = {
      getItem: () => {
        throw new Error('denied');
      },
      setItem: () => {
        throw new Error('quota');
      },
    };
    expect(readThemePaint(broken)).toBeNull();
    expect(writeThemePaint(manual, broken)).toBe(false);
  });
});

describe('pickPaintSide', () => {
  it('picks the side the OS shows while the theme follows it', () => {
    expect(pickPaintSide(following, true)).toBe(dark);
    expect(pickPaintSide(following, false)).toBe(light);
  });

  it('keeps the manual pick whatever the OS shows', () => {
    expect(pickPaintSide(manual, true)).toBe(light);
    expect(pickPaintSide(manual, false)).toBe(light);
  });

  it('picks the side of the daylight last shown while the theme follows the game', () => {
    expect(pickPaintSide(game, false)).toBe(dark);
    expect(pickPaintSide({ ...game, phase: 'day' }, true)).toBe(light);
  });
});

describe('parseThemePaint', () => {
  it('turns away anything that is not a paint', () => {
    const bad: (string | null)[] = [
      null,
      '',
      'not json',
      'null',
      '42',
      '[]',
      JSON.stringify({ ...manual, v: 2 }),
      JSON.stringify({ v: 1, follow: 'yes', manual: light }),
      JSON.stringify({ v: 1, follow: false }),
      JSON.stringify({ v: 1, follow: true, light }),
      JSON.stringify({ v: 1, follow: false, manual: { ...light, appearance: 'dim' } }),
      JSON.stringify({ v: 1, follow: false, manual: { ...light, id: 7 } }),
      JSON.stringify({ v: 1, follow: false, manual: { ...light, vars: null } }),
      JSON.stringify({ v: 1, follow: false, manual: { ...light, vars: ['--bg'] } }),
      JSON.stringify({ v: 1, follow: false, manual: { ...light, vars: { '--bg': 3 } } }),
      JSON.stringify({ v: 1, follow: false, manual: { ...light, vars: { color: 'red' } } }),
      JSON.stringify({ v: 1, follow: false, manual: { ...light, vars: {} } }),
      JSON.stringify({ ...game, phase: 'dusk' }),
      JSON.stringify({ ...game, phase: undefined }),
      JSON.stringify({ v: 1, follow: 'game', day: light, phase: 'day' }),
    ];
    for (const raw of bad) expect(parseThemePaint(raw), String(raw)).toBeNull();
  });

  it('reads a well formed paint', () => {
    expect(parseThemePaint(JSON.stringify(following))).toEqual(following);
    expect(parseThemePaint(JSON.stringify(game))).toEqual(game);
  });
});

describe('prepaintTheme', () => {
  it('paints the cached side on the root before anything else runs', () => {
    const root = fakeRoot();
    const storage = memoryStorage({ [THEME_PAINT_KEY]: JSON.stringify(following) });
    const side = prepaintTheme({
      storage: () => storage,
      systemDark: () => false,
      root: () => root,
    });
    expect(side).toEqual(light);
    expect(root.attrs).toEqual({ 'data-theme': 'rubric', 'data-appearance': 'light' });
    expect(root.vars).toEqual(light.vars);
  });

  it('follows the OS for the side it paints', () => {
    const root = fakeRoot();
    const storage = memoryStorage({ [THEME_PAINT_KEY]: JSON.stringify(following) });
    prepaintTheme({ storage: () => storage, systemDark: () => true, root: () => root });
    expect(root.attrs['data-appearance']).toBe('dark');
    expect(root.vars['--bg']).toBe('#1a1b26');
  });

  it('paints the daylight last shown and keeps it for the theme to hold', () => {
    const root = fakeRoot();
    const storage = memoryStorage({ [THEME_PAINT_KEY]: JSON.stringify(game) });
    prepaintTheme({ storage: () => storage, systemDark: () => false, root: () => root });
    expect(root.attrs['data-theme']).toBe('tokyo-night');
    expect(bootPaintPhase()).toBe('night');
    // A cache without the game leaves no daylight.
    storage.setItem(THEME_PAINT_KEY, JSON.stringify(following));
    prepaintTheme({ storage: () => storage, systemDark: () => false, root: () => root });
    expect(bootPaintPhase()).toBeNull();
  });

  it('leaves the stylesheet defaults alone without a good cache', () => {
    for (const raw of [undefined, '{broken', JSON.stringify({ v: 9 })]) {
      const root = fakeRoot();
      const storage = memoryStorage(raw === undefined ? {} : { [THEME_PAINT_KEY]: raw });
      const side = prepaintTheme({
        storage: () => storage,
        systemDark: () => false,
        root: () => root,
      });
      expect(side).toBeNull();
      expect(root.attrs).toEqual({});
      expect(root.vars).toEqual({});
    }
  });

  it('never throws when storage or the OS query fails', () => {
    const root = fakeRoot();
    expect(
      prepaintTheme({
        storage: () => {
          throw new Error('SecurityError');
        },
        systemDark: () => false,
        root: () => root,
      }),
    ).toBeNull();
    const storage = memoryStorage({ [THEME_PAINT_KEY]: JSON.stringify(following) });
    expect(
      prepaintTheme({
        storage: () => storage,
        systemDark: () => {
          throw new Error('no matchMedia');
        },
        root: () => root,
      }),
    ).toBeNull();
  });
});
