// Braille trend grid used by the `bar_layout: 'with_history'` mode of
// the vitals bar. Renders a single row of braille where each cell
// carries 2 samples (one per dot column) and vertical fill goes
// bottom-up across 4 dot rows of resolution. The live VitalsBar and
// the Settings preview both render this so the preview accurately
// reflects what the live bar will look like.

const SPARK_DOTS_LEFT_TOP_DOWN = [0x01, 0x02, 0x04, 0x40];
const SPARK_DOTS_RIGHT_TOP_DOWN = [0x08, 0x10, 0x20, 0x80];
const SPARK_ROWS = 1;
const SPARK_TOTAL_DOTS = SPARK_ROWS * 4;
const SPARK_BASE_CHAR = '⣿';

export function sparkBaseString(width: number): string {
  const line = SPARK_BASE_CHAR.repeat(width);
  return Array(SPARK_ROWS).fill(line).join('\n');
}

export function sparkTraceString(values: number[], width: number): string {
  const need = width * 2;
  const samples = values.slice(-need);
  // Pad missing history with the OLDEST known sample (or 0 if the
  // ring is still empty). Padding with 0 makes the leftmost cells
  // render as empty braille which lets the gray base show through —
  // reads as "your hp was 0% earlier" even when the user just
  // started recording and has been steady at 100%. Padding with
  // the oldest known value avoids that misleading gap.
  const pad = samples.length > 0 ? samples[0] : 0;
  while (samples.length < need) samples.unshift(pad);
  const lines: string[] = [];
  for (let row = 0; row < SPARK_ROWS; row++) {
    let line = '';
    for (let i = 0; i < width; i++) {
      const v0 = samples[i * 2];
      const v1 = samples[i * 2 + 1];
      const lit0 = Math.max(0, Math.min(SPARK_TOTAL_DOTS, Math.round(v0 * SPARK_TOTAL_DOTS)));
      const lit1 = Math.max(0, Math.min(SPARK_TOTAL_DOTS, Math.round(v1 * SPARK_TOTAL_DOTS)));
      let bits = 0;
      for (let d = 0; d < 4; d++) {
        const globalDot = row * 4 + d;
        const fromBottom = SPARK_TOTAL_DOTS - 1 - globalDot;
        if (fromBottom < lit0) bits |= SPARK_DOTS_LEFT_TOP_DOWN[d];
        if (fromBottom < lit1) bits |= SPARK_DOTS_RIGHT_TOP_DOWN[d];
      }
      line += String.fromCodePoint(0x2800 | bits);
    }
    lines.push(line);
  }
  return lines.join('\n');
}

/** Grid runs at the bar's font-size with -2px letter-spacing. The
 *  braille glyphs render in a fallback font (JetBrains Mono lacks
 *  braille) which is often noticeably wider than the `ch` unit
 *  used to size the container. We deliberately under-shoot here
 *  (1.0× cells) so the grid never visually overflows past the
 *  bar's right edge — a one-cell gap on the right is far less
 *  jarring than the trend grid sticking out past the bar. */
export function sparkCellCount(cells: number): number {
  return Math.max(8, cells);
}
