import { describe, expect, it } from 'vitest';
import { caretTop } from './useCaret';

// The caret is one cell of your terminal font, 1.2em tall to the pixel
// (Q16), and centres on its line at that height.

describe('caretTop', () => {
  it('centres a 17 px caret on a 14 px line, 1 px higher than the old 15', () => {
    // 14 px at line-height 1.35 gives the marker an 18.9 px line box.
    expect(caretTop(2, 18.9, 14)).toBeCloseTo(2.95);
    expect(caretTop(2, 18.9, 14)).toBeCloseTo(2 + (18.9 - 15) / 2 - 1);
  });

  it('follows the font size', () => {
    expect(caretTop(0, 21.6, 16)).toBeCloseTo((21.6 - 19) / 2);
    expect(caretTop(0, 27, 20)).toBeCloseTo((27 - 24) / 2);
  });
});
