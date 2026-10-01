import { afterEach, describe, expect, it, vi } from 'vitest';
import type { PieceSpan, RegionOnScreen } from './promptPointer';
import { installPromptPointerProbe, probePoint, type ProbeDeps } from './promptPointerProbe';

const span = (piece: number, row: number, col: number, width: number): PieceSpan => ({
  piece,
  row,
  col,
  width,
});

// The open row `<765> 800`, three pieces, on screen row 8 at 8 by 20
// cells from the grid's top left at (16, 50).
const open = {
  gen: 2,
  plain: '<765> 800',
  spans: [span(0, 0, 0, 1), span(1, 0, 1, 3), span(3, 0, 6, 3)],
};
const region: RegionOnScreen = { gen: 2, row: 8, col: 0, cols: 80, atBottom: true };

function deps(patch: Partial<ProbeDeps> = {}): ProbeDeps {
  return {
    renderer: () => 'xterm',
    terminal: () => ({
      promptRegion: () => Promise.resolve(region),
      cellAt: (x, y) =>
        x < 16 || y < 50 ? null : { row: Math.floor((y - 50) / 20), col: Math.floor((x - 16) / 8) },
    }),
    openRow: () => Promise.resolve(open),
    dock: () => null,
    band: () => null,
    ...patch,
  };
}

/** The point at the middle of screen cell `row`, `col`. */
const at = (row: number, col: number): [number, number] => [16 + col * 8 + 4, 50 + row * 20 + 10];

describe('the dev console probe of the pointer mapping', () => {
  it('names the piece under a point on your prompt in the text', async () => {
    expect(await probePoint(...at(8, 2), deps())).toEqual({
      where: 'text',
      renderer: 'xterm',
      cell: { row: 8, col: 2 },
      gen: 2,
      piece: 1,
      text: '765',
    });
    expect(await probePoint(...at(8, 7), deps({ renderer: () => 'native' }))).toMatchObject({
      renderer: 'native',
      piece: 3,
      text: '800',
    });
  });

  it('names no piece on the earlier prompt above, nor once the row closed', async () => {
    expect(await probePoint(...at(5, 2), deps())).toMatchObject({ piece: null, text: null });
    const closed = deps({
      terminal: () => ({ ...deps().terminal()!, promptRegion: () => Promise.resolve(null) }),
    });
    expect(await probePoint(...at(8, 2), closed)).toMatchObject({ gen: null, piece: null });
    expect(
      await probePoint(...at(8, 2), deps({ openRow: () => Promise.resolve(null) })),
    ).toMatchObject({
      piece: null,
    });
  });

  it('reads the dock own grid on the pinned band', async () => {
    const dock = () => ({
      left: 16,
      top: 600,
      right: 816,
      bottom: 660,
      zone: 2,
      cell: { width: 8, height: 20, cols: 100 },
    });
    const band = () => ({
      text: 'Tester: [===]\r\n<765>',
      spans: [span(0, 1, 0, 1), span(1, 1, 1, 3)],
    });
    // The band's second row starts the gap, the 2 px outset and a row
    // down the dock: 26 + 2 + 20 px.
    const hit = await probePoint(16 + 2 * 8 + 4, 600 + 28 + 20 + 10, deps({ dock, band }));
    expect(hit).toEqual({
      where: 'band',
      cell: { row: 1, col: 2 },
      piece: 1,
      text: '765',
    });
    expect(await probePoint(16 + 4, 600 + 28 + 10, deps({ dock, band }))).toMatchObject({
      where: 'band',
      piece: null,
    });
  });

  it('says nothing for a point off the terminal and the dock', async () => {
    expect(await probePoint(2, 2, deps())).toBeNull();
    expect(await probePoint(...at(8, 2), deps({ terminal: () => null }))).toBeNull();
  });
});

describe('the probe on window', () => {
  type Host = {
    addEventListener: ReturnType<typeof vi.fn>;
    removeEventListener: ReturnType<typeof vi.fn>;
    __voshPromptPointer?: (on?: boolean) => string;
  };
  const real = globalThis.window;
  afterEach(() => {
    globalThis.window = real;
  });

  it('listens for clicks while on and goes away when removed', () => {
    const host: Host = { addEventListener: vi.fn(), removeEventListener: vi.fn() };
    globalThis.window = host as unknown as Window & typeof globalThis;
    const remove = installPromptPointerProbe(deps());
    expect(host.__voshPromptPointer?.(true)).toBe(
      'The prompt pointer probe is on. Click parts of your prompt.',
    );
    expect(host.addEventListener).toHaveBeenCalledWith('pointerdown', expect.any(Function), true);
    expect(host.__voshPromptPointer?.(false)).toBe('The prompt pointer probe is off.');
    expect(host.removeEventListener).toHaveBeenCalledTimes(1);
    remove();
    expect(host.__voshPromptPointer).toBeUndefined();
  });
});
