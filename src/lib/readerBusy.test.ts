import { describe, expect, it } from 'vitest';
import { readerTracker } from './readerBusy';

describe('readerTracker', () => {
  it('sends whether any part holds, only when that changes', () => {
    const sent: boolean[] = [];
    const reader = readerTracker((busy) => sent.push(busy));
    // The first note sends, so a window that loads again sets the
    // session straight.
    reader.note('split', false);
    expect(sent).toEqual([false]);
    reader.note('liveSelection', true);
    reader.note('split', true);
    reader.note('liveSelection', true);
    expect(sent).toEqual([false, true]);
    // Busy until the last part lets go.
    reader.note('liveSelection', false);
    expect(sent).toEqual([false, true]);
    reader.note('split', false);
    expect(sent).toEqual([false, true, false]);
    reader.note('historySelection', false);
    expect(sent).toEqual([false, true, false]);
    reader.note('liveBack', true);
    expect(sent).toEqual([false, true, false, true]);
  });
});
