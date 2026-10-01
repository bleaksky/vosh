import { describe, expect, it } from 'vitest';
import { measureCell, subscribeFontLoads } from './useCellWidth';

/** A FontFaceSet as far as the hook reads one. */
function fakeFonts() {
  let settle: () => void = () => undefined;
  const target = new EventTarget();
  return {
    set: Object.assign(target, { ready: new Promise<void>((r) => (settle = r)) }),
    settle: () => settle(),
    load: () => target.dispatchEvent(new Event('loadingdone')),
  };
}

describe('the cell width of the terminal face', () => {
  it('measures ten cells of the face at 13 px', () => {
    const fonts: string[] = [];
    const width = measureCell('"JetBrainsMono Bundled", Menlo', () => ({
      set font(value: string) {
        fonts.push(value);
      },
      measureText: (text: string) => ({ width: text.length * 7.8 }),
    }));
    expect(width).toBe(7.8);
    expect(fonts).toEqual(['13px "JetBrainsMono Bundled", Menlo']);
  });

  it('falls back to 7.8 where nothing can measure', () => {
    expect(measureCell('Menlo', () => null)).toBe(7.8);
    expect(measureCell('Menlo', () => ({ font: '', measureText: () => ({ width: 0 }) }))).toBe(7.8);
  });

  it('measures again once the fonts load, since a face still loading measures as its fallback', async () => {
    const fonts = fakeFonts();
    let heard = 0;
    const stop = subscribeFontLoads(fonts.set, () => heard++);
    fonts.load();
    expect(heard).toBe(1);
    fonts.settle();
    await fonts.set.ready;
    await Promise.resolve();
    expect(heard).toBe(2);
    stop();
    fonts.load();
    expect(heard).toBe(2);
  });
});
