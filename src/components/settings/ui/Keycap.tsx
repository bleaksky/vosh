import type { ReactNode } from 'react';
import { cx } from './cx';

/** A keycap: 20 high (and at least 20 wide), radius 4, a 1 px ring
 *  (white 14%, black 12% on light themes), no fill, 11 px glyph in the
 *  secondary color. Set a row of them 4 px apart. Render the glyphs
 *  from shortcutKeys in src/lib/palette.ts so macOS reads ⌘ and the
 *  other systems read Ctrl. */
export function Keycap({ children, className }: { children: ReactNode; className?: string }) {
  return <kbd className={cx('st-keycap', className)}>{children}</kbd>;
}
