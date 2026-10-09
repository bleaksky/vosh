import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { BandEnv } from '../terminal/bandCells';
import { parseSgrCells } from '../terminal/sgrCells';
import { CellLine } from './PromptCells';

const ENV: BandEnv = {
  palette: Array.from({ length: 16 }, () => '#888888'),
  fg: '#e5e9f0',
  bg: '#2e3440',
  selection: '#4c566a',
  selectionText: '#eceff4',
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
    expect(html).toContain('<span class="visually-hidden">(Wizi 60) [1020/1020hp]</span>');
    const glyphs = [...html.matchAll(/<span class="pc-cells-glyph"[^>]*>/g)].map((m) => m[0]);
    expect(glyphs.length).toBeGreaterThan(0);
    expect(glyphs.every((g) => g.includes('aria-hidden="true"'))).toBe(true);
  });

  it('draws a run of full blocks as one bar with no seam between its cells', () => {
    // A bar six cells full, then four empty, as the Bars preset draws it.
    const html = line(`hp \x1b[32m${'█'.repeat(6)}\x1b[90m${'░'.repeat(4)}\x1b[0m`);
    const blocks = [...html.matchAll(/<span class="pc-cells-glyph pc-cells-block"[^>]*>/g)];
    expect(blocks).toHaveLength(1);
    const block = blocks[0][0];
    expect(block).toContain('left:23.4px');
    expect(block).toContain('width:46.8px');
    // The run's own color fills the font's height under its glyphs.
    expect(html).toMatch(/pc-cells-block"[^>]*><span style="background:#888888">██████<\/span>/);
    // Each empty cell stays a glyph of its own.
    const shades = [...html.matchAll(/<span class="pc-cells-glyph"[^>]*>░<\/span>/g)];
    expect(shades).toHaveLength(4);
  });

  it('draws a marked value too dim on the selection in the selection text', () => {
    const html = renderToStaticMarkup(
      <CellLine
        cells={parseSgrCells('\x1b[38;5;240m60\x1b[0m hp')[0] ?? []}
        env={ENV}
        cellW={7.8}
        marks={[{ from: 0, to: 2, warn: false }]}
      />,
    );
    const color = (ch: string) => new RegExp(`color:([^;"]*)[^>]*>${ch}<`).exec(html)?.[1] ?? null;
    // 256 color 240 reads 1.04:1 on Nord's selection, so the marked 60
    // takes the selection text, and the unmarked hp keeps the text color.
    expect(color('6')).toBe('#eceff4');
    expect(color('h')).toBe('#e5e9f0');
  });

  it('reads as far as a cut sample shows, its ellipsis too', () => {
    const html = line('1020/1020hp 800/800mn 930/930mv', 100);
    expect(html).toContain('<span class="visually-hidden">1020/1020hp…</span>');
  });
});
