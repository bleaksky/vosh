import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { BandEnv } from '../../lib/bandCells';
import { parseSgrCells } from '../../lib/sgrCells';
import { CellLine } from './PromptCells';

const ENV: BandEnv = {
  palette: Array.from({ length: 16 }, () => '#888888'),
  fg: '#e5e9f0',
  bg: '#2e3440',
  renderer: 'xterm',
  brightBold: false,
};

const line = (ansi: string, column?: number) =>
  renderToStaticMarkup(
    <CellLine
      cells={parseSgrCells(ansi)[0] ?? []}
      env={ENV}
      cellW={7.8}
      {...(column === undefined ? {} : { column })}
    />,
  );

describe('a line drawn cell by cell', () => {
  it('reads as its text, spaces and all, with each glyph hidden from a reader', () => {
    const html = line('\x1b[38;5;240m(Wizi 60)\x1b[0m [1020/1020hp] ');
    expect(html).toContain('<span class="st-visually-hidden">(Wizi 60) [1020/1020hp]</span>');
    const glyphs = [...html.matchAll(/<span class="pc-cells-glyph"[^>]*>/g)].map((m) => m[0]);
    expect(glyphs.length).toBeGreaterThan(0);
    expect(glyphs.every((g) => g.includes('aria-hidden="true"'))).toBe(true);
  });

  it('reads as far as a cut sample shows, its ellipsis too', () => {
    const html = line('1020/1020hp 800/800mn 930/930mv', 100);
    expect(html).toContain('<span class="st-visually-hidden">1020/1020hp…</span>');
  });
});
