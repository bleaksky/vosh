import { describe, expect, it } from 'vitest';
import type { PromptRendered, PromptSpan } from '../ipc/promptDesign';
import type { VitalsText } from '../ipc/vitals';
import { parseSgrCells, type Cell } from '../terminal/sgrCells';
import { fitText, textRows, vitalsTextLines, wrapAt, type TextLine } from './vitalsTextFit';

// Vosh's vitals text as the session renders it (crates/prompt vitals.rs
// over Preview::Fight with a Blackwatch guard at 54 and Tolliver at 765
// of 1020), 23 cells wide, the 200 pt panel of board 6. Piece 2 is the
// %{right}, and only its span matters here.

const FIGHT_LIVE =
  'a Blackwatch guard  \x1b[33m54%\x1b[39m\r\n\x1b[39m765\x1b[90m/1020hp\x1b[39m 800\x1b[90m/800mn\x1b[39m 930\x1b[90m/930mv\x1b[39m\x1b[0m';
const FIGHT_FULL =
  'a Blackwatch guard \x1b[33m100%\x1b[39m\r\n\x1b[39m1020\x1b[90m/1020hp\x1b[39m 800\x1b[90m/800mn\x1b[39m 930\x1b[90m/930mv\x1b[39m\x1b[0m';
const HEALTHY =
  '\x1b[39m1020\x1b[90m/1020hp\x1b[39m 800\x1b[90m/800mn\x1b[39m 930\x1b[90m/930mv\x1b[39m\x1b[0m';
const LONG_LIVE =
  'a young Liaison Officer of the White Wolf \x1b[33m54%\x1b[39m\r\n\x1b[39m765\x1b[90m/1020hp\x1b[39m 800\x1b[90m/800mn\x1b[39m 930\x1b[90m/930mv\x1b[39m\x1b[0m';

const RIGHT = 2;

function push(row: number, col: number, width: number): PromptSpan {
  const plain = { kind: 'default' } as const;
  return {
    piece: RIGHT,
    row,
    col,
    width,
    fg: plain,
    bg: plain,
    bold: false,
    italic: false,
    underline: false,
  };
}

function rendered(ansi: string, spans: PromptSpan[] = []): PromptRendered {
  const rows = parseSgrCells(ansi);
  return {
    ansi,
    plain: rows.map(text).join('\n'),
    rows: ansi === '' ? 0 : rows.length,
    spans,
  };
}

function text(cells: readonly Cell[]): string {
  return cells.map((c) => c.ch).join('');
}

/** Each line as its text, the right part after a bar. */
function shown(lines: readonly TextLine[]): string[] {
  return lines.map((l) => (l.right ? `${text(l.left)}|${text(l.right)}` : text(l.left)));
}

const FIGHT: VitalsText = {
  session: 1,
  live: rendered(FIGHT_LIVE, [push(0, 18, 2)]),
  full: rendered(FIGHT_FULL, [push(0, 18, 1)]),
  fight: [true, false],
  right: [RIGHT],
};

const CALM: VitalsText = {
  session: 1,
  live: rendered(HEALTHY),
  full: rendered(HEALTHY),
  fight: [false],
  right: [RIGHT],
};

describe('fitting a vitals text to the footer', () => {
  it('wraps a row at its spaces, a word too wide on a line of its own', () => {
    const [row] = parseSgrCells('765/1020hp 800/800mn 930/930mv');
    expect(wrapAt(row, 30)).toEqual([]);
    expect(wrapAt(row, 23)).toEqual([2]);
    expect(wrapAt(row, 10)).toEqual([1, 2]);
    expect(wrapAt(row, 4)).toEqual([1, 2]);
  });

  it('breaks at the same space healthy and in a fight, as the full text does', () => {
    // At 20 cells your vitals at 765 fit two to a line, but at full
    // they do not, so the fight breaks where full health breaks.
    const live = textRows(FIGHT.live, FIGHT.right);
    const full = textRows(FIGHT.full, FIGHT.right);
    const own = fitText(live, live, 20);
    expect(shown(own[1])).toEqual(['765/1020hp 800/800mn', '930/930mv']);
    const fight = fitText(live, full, 20);
    expect(shown(fight[1])).toEqual(['765/1020hp', '800/800mn 930/930mv']);
    expect(shown(vitalsTextLines(CALM, 20, false))).toEqual(['1020/1020hp', '800/800mn 930/930mv']);
  });

  it('draws board 6 at 200 pt, the opponent whole and your vitals on two lines', () => {
    expect(shown(vitalsTextLines(FIGHT, 23, false))).toEqual([
      'a Blackwatch guard|54%',
      '765/1020hp 800/800mn',
      '930/930mv',
    ]);
    expect(shown(vitalsTextLines(CALM, 23, false))).toEqual(['1020/1020hp 800/800mn', '930/930mv']);
  });

  it('keeps the right part of a row with %{right} whole and never wraps it', () => {
    const long: VitalsText = {
      ...FIGHT,
      live: rendered(LONG_LIVE, [push(0, 41, 1)]),
      full: rendered(LONG_LIVE, [push(0, 41, 1)]),
    };
    const [opponent] = vitalsTextLines(long, 37, false);
    expect(text(opponent.left)).toBe('a young Liaison Officer of the White Wolf');
    expect(text(opponent.right ?? [])).toBe('54%');
  });

  it('keeps only the rows that read your fight while your prompt hides your vitals', () => {
    expect(shown(vitalsTextLines(FIGHT, 23, true))).toEqual(['a Blackwatch guard|54%']);
    expect(vitalsTextLines(CALM, 23, true)).toEqual([]);
  });

  it('keeps the look of each cell', () => {
    const [opponent] = vitalsTextLines(FIGHT, 23, false);
    expect(opponent.right?.[0].attrs.fg).toEqual({ kind: 'named', n: 3 });
    const vitals = vitalsTextLines(FIGHT, 23, false)[1];
    expect(vitals.left[3].attrs.fg).toEqual({ kind: 'named', n: 8 });
  });

  it('draws nothing for a text that renders nothing', () => {
    expect(vitalsTextLines({ ...CALM, live: rendered(''), full: rendered('') }, 23, false)).toEqual(
      [],
    );
  });
});
