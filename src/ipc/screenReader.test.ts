import { describe, expect, it, vi } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import {
  DEFAULT_SCREEN_READER,
  normalizeScreenReader,
  onScreenReader,
  subscribeScreenReaderChanged,
} from './screenReader';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

describe('onScreenReader', () => {
  it('hands its listener what one read gave the reader and the session apart', async () => {
    vi.mocked(listen).mockClear();
    const heard = vi.fn();
    await onScreenReader(heard);
    const [event, handler] = vi.mocked(listen).mock.calls[0];
    expect(event).toBe('session://screen-reader');
    const feed = {
      lines: ['You are thirsty.', 'You are hungry.'],
      count: 2,
      prompt: null,
      away: true,
    };
    handler({ event, id: 1, payload: { session: 2, ...feed } });
    expect(heard).toHaveBeenCalledWith(feed, 2);
  });
});

describe('the screen reader choices', () => {
  it('read off and a burst of 8 until you make any', () => {
    expect(DEFAULT_SCREEN_READER).toEqual({
      screen_reader: false,
      screen_reader_background: false,
      screen_reader_prompt: false,
      screen_reader_burst: 8,
    });
    expect(normalizeScreenReader(null)).toEqual(DEFAULT_SCREEN_READER);
    expect(normalizeScreenReader({ screen_reader: 'yes', screen_reader_burst: 12 })).toEqual(
      DEFAULT_SCREEN_READER,
    );
  });

  it('come off the bus as Settings sent them', async () => {
    vi.mocked(listen).mockClear();
    const heard = vi.fn();
    await subscribeScreenReaderChanged(heard);
    const [event, handler] = vi.mocked(listen).mock.calls[0];
    expect(event).toBe('vosh://screen-reader-changed');
    const sent = {
      screen_reader: true,
      screen_reader_background: true,
      screen_reader_prompt: false,
      screen_reader_burst: 16,
    };
    handler({ event, id: 1, payload: sent });
    expect(heard).toHaveBeenCalledWith(sent);
  });
});
