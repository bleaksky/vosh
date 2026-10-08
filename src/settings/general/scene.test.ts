import { describe, expect, it } from 'vitest';
import type { LogSession } from '../../ipc/logs';
import {
  addable,
  clockAfter,
  endingAt,
  FIRST_FILTER,
  firstRange,
  keptText,
  logsWorld,
  parseClock,
  SCENE_CHANNELS,
  startingAt,
  withFrom,
  withTo,
} from './scene';

/** A local time on October 3, 2026, or the next day with `day` 4. */
const at = (h: number, m: number, s = 0, day = 3) => new Date(2026, 9, day, h, m, s).getTime();

const log = (started: number, ended: number | null): LogSession => ({
  id: 7,
  host: 'play.theforsakenlands.com',
  port: 1848,
  started_at_ms: started,
  ended_at_ms: ended,
  line_count: 1284,
});

describe('the range a scene opens on', () => {
  it('takes the last 15 minutes of the log, whole minutes', () => {
    expect(firstRange(log(at(20, 0), at(21, 15, 30)))).toEqual({
      log: 7,
      fromMs: at(21, 0),
      toMs: at(21, 15, 59) + 999,
    });
  });

  it('starts no earlier than the log, and a log that runs ends now', () => {
    expect(firstRange(log(at(21, 2, 10), null), at(21, 15, 30))).toEqual({
      log: 7,
      fromMs: at(21, 2),
      toMs: at(21, 15, 59) + 999,
    });
  });
});

describe('From and To', () => {
  it('reads a 24 hour time', () => {
    expect(parseClock('21:14')).toEqual({ hours: 21, minutes: 14 });
    expect(parseClock(' 9:05 ')).toEqual({ hours: 9, minutes: 5 });
    expect(parseClock('9.05')).toEqual({ hours: 9, minutes: 5 });
    expect(parseClock('24:00')).toBeNull();
    expect(parseClock('21:6')).toBeNull();
    expect(parseClock('soon')).toBeNull();
  });

  it('lands past midnight on the next day of a log that runs into it', () => {
    expect(clockAfter(at(23, 50), 0, 10)).toBe(at(0, 10, 0, 4));
    expect(clockAfter(at(23, 50, 30), 23, 50)).toBe(at(23, 50));
  });

  it('moves From on the log clock and keeps To after it', () => {
    const s = log(at(21, 2), at(21, 16));
    const range = firstRange(s);
    expect(withFrom(range, s, '21:14')).toEqual({
      log: 7,
      fromMs: at(21, 14),
      toMs: range.toMs,
    });
    expect(withFrom(range, s, '21:20')?.toMs).toBe(at(21, 20, 59) + 999);
    expect(withFrom(range, s, 'later')).toBeNull();
  });

  it('ends To on the last moment of its minute', () => {
    const range = { log: 7, fromMs: at(21, 14), toMs: at(21, 16, 59) + 999, toId: 140 };
    expect(withTo(range, '21:15')).toEqual({
      log: 7,
      fromMs: at(21, 14),
      toMs: at(21, 15, 59) + 999,
      toId: null,
    });
  });

  it('starts or ends on a line you click', () => {
    const range = firstRange(log(at(21, 2), at(21, 16)));
    expect(startingAt(range, 101, at(21, 14, 1))).toEqual({
      ...range,
      fromMs: at(21, 14),
      fromId: 101,
    });
    expect(endingAt(range, 119, at(21, 15, 2))).toEqual({
      ...range,
      toMs: at(21, 15, 59) + 999,
      toId: 119,
    });
  });
});

describe('what a scene leaves out', () => {
  it('leaves out prompts, commands and five channels at first (Q10)', () => {
    expect(FIRST_FILTER).toEqual({
      prompts: false,
      commands: false,
      leftOut: ['tell', 'newbie', 'pray', 'immortal', 'imp'],
    });
    expect(addable(FIRST_FILTER.leftOut)).toEqual([
      'say',
      'yell',
      'gtell',
      'cabal',
      'clan',
      'faction',
    ]);
    expect(addable(SCENE_CHANNELS)).toEqual([]);
  });

  it('counts the lines it keeps', () => {
    expect(keptText(12, 20)).toBe('12 of 20 lines');
    expect(keptText(1, 1)).toBe('1 of 1 line');
    expect(keptText(0, 0)).toBe('No lines');
    expect(keptText(1200, 2400)).toBe('1,200 of 2,400 lines');
  });

  it('follows Log sessions, which logs no connection to this computer until you choose', () => {
    expect(logsWorld(null, 'play.theforsakenlands.com')).toBe(true);
    expect(logsWorld(null, 'localhost')).toBe(false);
    expect(logsWorld(true, '127.0.0.1')).toBe(true);
    expect(logsWorld(false, 'play.theforsakenlands.com')).toBe(false);
  });
});
