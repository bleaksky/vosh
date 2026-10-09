// What sizes the font of a Size row draws at, so the row offers half
// sizes only for a font that takes them (src/lib/textSize.ts). Rust
// reads the font file once for each family, and the answer stays for
// the life of the window.

import { useEffect, useState } from 'react';
import { fontSizing } from '../ipc/uiConfig';
import { renderFontStack } from '../lib/fontLoader';
import { SCALABLE, type FontSizing } from '../lib/textSize';
import { primaryFontFamily } from '../theme/appearanceSettings';

// The CSS generic families, which the browser draws with a system font
// that scales.
const GENERICS = new Set(['monospace', 'ui-monospace', 'sans-serif', 'serif', 'system-ui']);

const cache = new Map<string, Promise<FontSizing>>();

/** The family Rust judges for the font list `stack`, the one the list
 *  draws with first. Null for no list, the bundled JetBrains Mono and a
 *  generic family, which all scale. */
export function sizingFamily(stack: string | null): string | null {
  if (stack === null || stack.trim() === '') return null;
  const family = primaryFontFamily(renderFontStack(stack));
  if (family === '' || /\sbundled$/i.test(family) || GENERICS.has(family.toLowerCase())) {
    return null;
  }
  return family;
}

/** The sizes the font list `stack` draws at, asked once for each
 *  family. */
export function loadFontSizing(stack: string | null): Promise<FontSizing> {
  const family = sizingFamily(stack);
  if (family === null) return Promise.resolve(SCALABLE);
  let found = cache.get(family);
  if (!found) {
    found = fontSizing(family);
    cache.set(family, found);
  }
  return found;
}

/** The sizes the font list `stack` draws at. Half sizes until Rust
 *  answers, which is the answer for most fonts. */
export function useFontSizing(stack: string | null): FontSizing {
  const family = sizingFamily(stack);
  const [sizing, setSizing] = useState<{ family: string | null; sizing: FontSizing }>({
    family: null,
    sizing: SCALABLE,
  });
  useEffect(() => {
    let live = true;
    void loadFontSizing(stack).then((next) => {
      if (live) setSizing({ family, sizing: next });
    });
    return () => {
      live = false;
    };
  }, [stack, family]);
  return sizing.family === family ? sizing.sizing : SCALABLE;
}
