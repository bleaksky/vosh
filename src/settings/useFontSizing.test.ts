import { describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() =>
  vi.fn((_cmd: string, args?: { family?: string }) =>
    Promise.resolve(
      args?.family === 'Old Bitmap'
        ? { half_sizes: false, strikes: [13] }
        : { half_sizes: true, strikes: [] },
    ),
  ),
);
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const { loadFontSizing, sizingFamily } = await import('./useFontSizing');

describe('sizingFamily', () => {
  it('asks about the family a font list draws with first', () => {
    expect(sizingFamily('"Old Bitmap", Menlo, monospace')).toBe('Old Bitmap');
    expect(sizingFamily('Menlo, monospace')).toBe('Menlo');
  });

  it('asks nothing for the bundled font, a generic family or no list', () => {
    expect(sizingFamily('"JetBrainsMono Bundled", Menlo, monospace')).toBeNull();
    expect(sizingFamily('monospace')).toBeNull();
    expect(sizingFamily('')).toBeNull();
    expect(sizingFamily(null)).toBeNull();
  });
});

describe('loadFontSizing', () => {
  it('asks Rust once for each family', async () => {
    invoke.mockClear();
    expect(await loadFontSizing('"Old Bitmap", monospace')).toEqual({
      half_sizes: false,
      strikes: [13],
    });
    expect(await loadFontSizing('"Old Bitmap", Menlo')).toEqual({
      half_sizes: false,
      strikes: [13],
    });
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('font_sizing', { family: 'Old Bitmap' });
  });

  it('takes half sizes for the bundled font without asking', async () => {
    invoke.mockClear();
    expect((await loadFontSizing('"JetBrainsMono Bundled", monospace')).half_sizes).toBe(true);
    expect(invoke).not.toHaveBeenCalled();
  });
});
