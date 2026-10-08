import { describe, expect, it } from 'vitest';
import { CANONICAL_ANSI_16 } from '../../theme/baseAnsi';
import {
  groupLogDays,
  logColorCss,
  logCountText,
  logDay,
  logEmptyText,
  isLocalHost,
  logFileName,
  logMatcher,
  logPalette,
  logPlaceholder,
  logRangeScope,
  LOG_RANGES,
  logSpanCss,
  logSessionLabel,
  logTime,
  markMatches,
  matchRanges,
  parseLogLine,
  savedLogsText,
} from './logView';
import { findTheme } from '../../theme/themes';

// Local times, so the tests hold in any time zone.
const at = (month: number, day: number, hour: number, minute: number, year = 2026) =>
  new Date(year, month - 1, day, hour, minute).getTime();
const NOW = at(9, 29, 5, 40);

describe('savedLogsText', () => {
  it('counts logs and lines on this computer', () => {
    expect(savedLogsText(447, 708350, 'Mac')).toBe('447 logs and 708,350 lines on this Mac.');
    expect(savedLogsText(1, 1, 'PC')).toBe('1 log and 1 line on this PC.');
    expect(savedLogsText(0, 0, 'Mac')).toBe('Vosh has not saved a log on this Mac yet.');
    expect(savedLogsText(0, 0, 'computer')).toBe('Vosh has not saved a log on this computer yet.');
  });
});

describe('isLocalHost', () => {
  it('reads a host the way vosh-log does', () => {
    expect(isLocalHost('127.0.0.1')).toBe(true);
    expect(isLocalHost(' LocalHost. ')).toBe(true);
    expect(isLocalHost('play.theforsakenlands.com')).toBe(false);
    expect(isLocalHost('localhost.example.org')).toBe(false);
    expect(isLocalHost('127.0.0.2')).toBe(false);
  });
});

describe('the ranges the view reads', () => {
  const world = { host: 'play.theforsakenlands.com', port: 9009 };

  it('opens on the last 7 days and offers the four ranges in order', () => {
    expect(LOG_RANGES.map((r) => r.label)).toEqual([
      'This session',
      'Last 7 days',
      'Last 30 days',
      'All time',
    ]);
  });

  it('reads the world the session dials over each range', () => {
    const day = 86_400_000;
    expect(logRangeScope('week', world, NOW)).toEqual({ ...world, sinceMs: NOW - 7 * day });
    expect(logRangeScope('month', world, NOW)).toEqual({ ...world, sinceMs: NOW - 30 * day });
    expect(logRangeScope('all', world, NOW)).toEqual(world);
    expect(logRangeScope('session', world, NOW)).toEqual({ ...world, thisSession: true });
  });

  it('names the file Save as file writes', () => {
    expect(logFileName('week', null)).toBe('Vosh log, last 7 days');
    expect(logFileName('session', null)).toBe('Vosh log, this session');
    expect(logFileName('all', null)).toBe('Vosh log, all time');
    expect(logFileName(null, at(10, 8, 7, 5))).toBe('Vosh log, 2026-10-08 07.05');
  });

  it('says what it searches and what it found nothing in', () => {
    expect(logPlaceholder('week')).toBe('Search the last 7 days');
    expect(logPlaceholder('session')).toBe('Search this session');
    expect(logPlaceholder('all')).toBe('Search every log');
    expect(logPlaceholder(null)).toBe('Search this log');
    expect(logEmptyText('month')).toBe('Nothing saved from this world in the last 30 days.');
    expect(logEmptyText('session')).toBe('This session has saved nothing since Vosh opened.');
    expect(logEmptyText(null)).toBe('This log has no saved lines.');
  });
});

describe('logCountText', () => {
  it('names the newest page while older matches wait', () => {
    expect(logCountText(500, 2423, 'Blackwatch')).toBe('Newest 500 of 2,423 lines');
    expect(logCountText(1000, 2423, 'Blackwatch')).toBe('Newest 1,000 of 2,423 lines');
  });

  it('counts every line once they are all loaded', () => {
    expect(logCountText(12, 12, 'x')).toBe('12 lines');
    expect(logCountText(1, 1, 'x')).toBe('1 line');
    expect(logCountText(3, null, 'x')).toBe('3 lines');
  });

  it('says when nothing matches', () => {
    expect(logCountText(0, 0, 'nothing')).toBe('No lines match');
    expect(logCountText(0, 0, '')).toBe('No saved lines');
  });
});

describe('times and days', () => {
  it('writes the hour without a leading zero on the 24 hour clock', () => {
    expect(logTime(at(9, 29, 3, 52))).toBe('3:52');
    expect(logTime(at(9, 28, 17, 8))).toBe('17:08');
    expect(logTime(at(9, 28, 0, 5))).toBe('0:05');
  });

  it('names today and writes other days by month', () => {
    expect(logDay(at(9, 29, 1, 39), NOW)).toBe('Today');
    expect(logDay(at(9, 28, 23, 59), NOW)).toBe('September 28');
    expect(logDay(at(9, 26, 1, 51), NOW)).toBe('September 26');
    expect(logDay(at(12, 31, 22, 0, 2025), NOW)).toBe('December 31, 2025');
  });

  it('labels a session by when it started', () => {
    expect(logSessionLabel(at(9, 29, 3, 52), NOW)).toBe('Today, 3:52');
    expect(logSessionLabel(at(9, 28, 17, 28), NOW)).toBe('September 28, 17:28');
  });

  it('groups lines under their day in order', () => {
    const lines = [
      { ts_ms: at(9, 26, 1, 51), id: 1 },
      { ts_ms: at(9, 26, 2, 14), id: 2 },
      { ts_ms: at(9, 29, 1, 39), id: 3 },
    ];
    const groups = groupLogDays(lines, NOW);
    expect(groups.map((g) => [g.day, g.lines.map((l) => l.id)])).toEqual([
      ['September 26', [1, 2]],
      ['Today', [3]],
    ]);
  });
});

describe('parseLogLine', () => {
  it('reads SGR colors as ANSI indexes', () => {
    const spans = parseLogLine('Fort is [\x1b[36mKNIGHT\x1b[0m] controlled.');
    expect(spans).toEqual([
      { text: 'Fort is [' },
      { text: 'KNIGHT', fg: 6 },
      { text: '] controlled.' },
    ]);
  });

  it('reads bold, bright, 256, and true color', () => {
    const spans = parseLogLine(
      new TextEncoder().encode(
        '(\x1b[1;37mWhite Aura\x1b[22;39m) \x1b[93ma\x1b[38;5;42mb\x1b[48;2;1;2;300mc',
      ),
    );
    expect(spans).toEqual([
      { text: '(' },
      { text: 'White Aura', bold: true, fg: 7 },
      { text: ') ', bold: false },
      { text: 'a', bold: false, fg: 11 },
      { text: 'b', bold: false, fg: 42 },
      { text: 'c', bold: false, fg: 42, bg: 'rgb(1, 2, 255)' },
    ]);
  });

  it('drops other escapes and control characters but keeps tabs', () => {
    expect(parseLogLine('\x1b[2Kone\r\x07\ttwo\x1b(B')).toEqual([{ text: 'one\ttwo' }]);
    expect(parseLogLine('')).toEqual([]);
  });
});

describe('colors', () => {
  it('uses the theme colors or the base palette', () => {
    const nord = findTheme('nord').xterm;
    expect(logPalette(nord, true, null)[6]).toBe(nord.cyan);
    expect(logPalette(nord, false, null)[6]).toBe(CANONICAL_ANSI_16.cyan);
    const base = Array.from({ length: 16 }, (_, i) => `#0000${String(i).padStart(2, '0')}`);
    expect(logPalette(nord, false, base)[15]).toBe('#000015');
  });

  it('draws a span the way the terminal does', () => {
    const palette = logPalette(findTheme('nord').xterm, true, null);
    // Bold white is bright white, in the bold face only with bright bold on.
    expect(logSpanCss({ bold: true, fg: 7 }, palette, true)).toEqual({
      color: palette[15],
      fontWeight: 700,
    });
    expect(logSpanCss({ bold: true, fg: 7 }, palette, false)).toEqual({ color: palette[15] });
    // Bold on a 256 color keeps the bold face.
    expect(logSpanCss({ bold: true, fg: 42 }, palette, false)).toEqual({
      color: 'rgb(0, 215, 135)',
      fontWeight: 700,
    });
    expect(logSpanCss({ fg: 6, italic: true, underline: true }, palette, false)).toEqual({
      color: palette[6],
      fontStyle: 'italic',
      textDecoration: 'underline',
    });
    expect(logSpanCss({ inverse: true, fg: 1 }, palette, false)).toEqual({
      color: 'var(--bg)',
      background: palette[1],
    });
  });

  it('draws the cube and gray ramp as xterm does', () => {
    const palette = logPalette(findTheme('nord').xterm, true, null);
    expect(logColorCss(4, palette)).toBe(palette[4]);
    expect(logColorCss(16, palette)).toBe('rgb(0, 0, 0)');
    expect(logColorCss(42, palette)).toBe('rgb(0, 215, 135)');
    expect(logColorCss(232, palette)).toBe('rgb(8, 8, 8)');
    expect(logColorCss('rgb(1, 2, 3)', palette)).toBe('rgb(1, 2, 3)');
  });
});

describe('matches', () => {
  it('reads the pattern as a regular expression, with or without case', () => {
    expect(matchRanges('Blackwatch and blackwatch', logMatcher('blackwatch', false))).toEqual([
      [0, 10],
      [15, 25],
    ]);
    expect(matchRanges('Blackwatch and blackwatch', logMatcher('blackwatch', true))).toEqual([
      [15, 25],
    ]);
    expect(matchRanges('a1b22', logMatcher('\\d+', false))).toEqual([
      [1, 2],
      [3, 5],
    ]);
  });

  it('marks nothing for an empty pattern, an empty match, or a pattern it cannot read', () => {
    expect(logMatcher('', false)).toBeNull();
    expect(logMatcher('(', false)).toBeNull();
    expect(matchRanges('abc', logMatcher('x*', false))).toEqual([]);
  });

  it('splits styled spans at the edges of each match', () => {
    const spans = parseLogLine('(\x1b[1mWhite Aura\x1b[0m) the Aura');
    const pieces = markMatches(spans, logMatcher('aura', false));
    expect(pieces.map((p) => [p.text, p.match, p.bold ?? false])).toEqual([
      ['(', false, false],
      ['White ', false, true],
      ['Aura', true, true],
      [') the ', false, false],
      ['Aura', true, false],
    ]);
  });

  it('carries a match across two spans', () => {
    const spans = parseLogLine('Black\x1b[36mwatch\x1b[0m Guard');
    const pieces = markMatches(spans, logMatcher('blackwatch', false));
    expect(pieces.map((p) => [p.text, p.match, p.fg])).toEqual([
      ['Black', true, undefined],
      ['watch', true, 6],
      [' Guard', false, undefined],
    ]);
  });
});
