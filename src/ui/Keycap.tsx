import type { ReactNode } from 'react';
import { cx } from './cx';

/** A keycap: 20 high (and at least 20 wide), radius 4, a 1 px ring
 *  (white 14%, black 12% on light themes), no fill, 11 px glyph in the
 *  secondary color. Set a row of them 4 px apart. Render the glyphs
 *  from shortcutKeys in src/lib/shortcuts.ts so macOS reads ⌘ and the
 *  other systems read Ctrl. */
export function Keycap({ children, className }: { children: ReactNode; className?: string }) {
  // One glyph, like ⇧ or ⌘, sits in a square 20 wide even when it is
  // wider than the padding leaves room for. A word like Ctrl grows.
  const glyph = typeof children === 'string' && [...children].length === 1;
  return <kbd className={cx('keycap', glyph && 'is-glyph', className)}>{children}</kbd>;
}
