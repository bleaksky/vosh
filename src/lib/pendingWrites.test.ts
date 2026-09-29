import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { listen, type EventCallback } from '@tauri-apps/api/event';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

import {
  commitFocusedField,
  createDebouncedWrite,
  createPendingWrites,
  FLUSH_REQUEST_EVENT,
  listenForQuitFlush,
  pendingWrites,
  runCloseRequest,
  sendPendingWrites,
  type CloseGuard,
} from './pendingWrites';

afterEach(() => {
  vi.useRealTimers();
});

/** A focused field that saves when you leave it, like NumberField. */
function focusedField(onLeave: () => void) {
  return { activeElement: { tagName: 'INPUT', blur: onLeave } };
}

describe('closing the Settings window', () => {
  it('sends the last edit before the window closes', async () => {
    // You changed the vitals density, then pressed Cmd+W within 250 ms.
    vi.useFakeTimers();
    const events: string[] = [];
    const writes = createPendingWrites();
    const autosave = createDebouncedWrite<string>(async (value) => {
      events.push(`saved ${value}`);
    });
    writes.register(() => autosave.flush());
    autosave.schedule('density line', 250);

    await runCloseRequest({
      send: () => sendPendingWrites({ writes }),
      guard: () => null,
      close: () => {
        events.push('closed');
      },
    });
    expect(events).toEqual(['saved density line', 'closed']);
    // The pause ends later with nothing left to send.
    await vi.advanceTimersByTimeAsync(500);
    expect(events).toEqual(['saved density line', 'closed']);
  });

  it('saves a number you typed but did not leave before closing', async () => {
    // You typed 320 into Width and closed the window with focus still in
    // the field. Leaving the field saves it, then the flush sends it.
    const events: string[] = [];
    const writes = createPendingWrites();
    const paneWrite = createDebouncedWrite<number>(async (width) => {
      events.push(`saved width ${width}`);
    });
    writes.register(() => paneWrite.flush());
    const doc = focusedField(() => paneWrite.schedule(320, 250));

    await runCloseRequest({
      send: () => sendPendingWrites({ writes, commitFocus: true, doc }),
      guard: () => null,
      close: () => {
        events.push('closed');
      },
    });
    expect(events).toEqual(['saved width 320', 'closed']);
  });

  it('sends the writes and then lets a page with unsaved changes ask', async () => {
    const events: string[] = [];
    let proceed: (() => void) | null = null;
    // Leaving the focused field is what leaves the page with unsaved
    // changes, so the guard is read after the writes.
    let guard: CloseGuard | null = null;
    await runCloseRequest({
      send: async () => {
        events.push('sent');
        guard = (go) => {
          events.push('asked');
          proceed = go;
        };
      },
      guard: () => guard,
      close: () => {
        events.push('closed');
      },
    });
    expect(events).toEqual(['sent', 'asked']);
    (proceed as (() => void) | null)?.();
    await Promise.resolve();
    expect(events).toEqual(['sent', 'asked', 'closed']);
  });

  it('still closes when a write fails', async () => {
    const close = vi.fn();
    const spy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    await runCloseRequest({
      send: () => Promise.reject(new Error('disk full')),
      guard: () => null,
      close,
    });
    expect(close).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });
});

describe('pending writes', () => {
  it('runs every flush and says when one failed', async () => {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const writes = createPendingWrites();
    const ran: string[] = [];
    writes.register(() => {
      ran.push('a');
    });
    writes.register(async () => {
      ran.push('b');
      throw new Error('refused');
    });
    writes.register(async () => {
      ran.push('c');
    });
    expect(await writes.flushAll()).toBe(false);
    expect(ran.sort()).toEqual(['a', 'b', 'c']);
    expect(logged).toHaveBeenCalledTimes(1);
    logged.mockRestore();
  });

  it('sends only the newest snapshot when two pages saved through one writer', async () => {
    // Layout and its status line row both save the whole config. The
    // newer snapshot holds both edits, and the older one must not land
    // after it.
    const sent: string[] = [];
    const writes = createPendingWrites();
    const autosave = createDebouncedWrite<string>(async (v) => {
      sent.push(v);
    });
    writes.register(() => autosave.flush());
    autosave.schedule('density', 250);
    autosave.schedule('density and tick style', 250);
    await writes.flushAll();
    expect(sent).toEqual(['density and tick style']);
  });

  it('gives up on a flush that never ends', async () => {
    vi.useFakeTimers();
    const writes = createPendingWrites();
    writes.register(() => new Promise<void>(() => undefined));
    const done = writes.flushAll(800);
    await vi.advanceTimersByTimeAsync(800);
    expect(await done).toBe(false);
  });

  it('drops a writer that goes away', async () => {
    const writes = createPendingWrites();
    const flush = vi.fn();
    const unregister = writes.register(flush);
    unregister();
    expect(await writes.flushAll()).toBe(true);
    expect(flush).not.toHaveBeenCalled();
  });
});

describe('a debounced write', () => {
  it('sends the latest value once after the pause', async () => {
    vi.useFakeTimers();
    const sent: number[] = [];
    const write = createDebouncedWrite<number>(async (v) => {
      sent.push(v);
    });
    write.schedule(1, 250);
    write.schedule(2, 250);
    await vi.advanceTimersByTimeAsync(249);
    expect(sent).toEqual([]);
    await vi.advanceTimersByTimeAsync(1);
    expect(sent).toEqual([2]);
    expect(write.hasPending()).toBe(false);
  });

  it('takes a patch while it waits and forgets a dropped value', async () => {
    vi.useFakeTimers();
    const sent: string[] = [];
    const write = createDebouncedWrite<string>(async (v) => {
      sent.push(v);
    });
    write.patch((v) => `${v}!`);
    write.schedule('nord', 250);
    write.patch((v) => `${v} dark`);
    await write.flush();
    expect(sent).toEqual(['nord dark']);
    write.schedule('vellum', 250);
    write.drop();
    await vi.advanceTimersByTimeAsync(500);
    await write.flush();
    expect(sent).toEqual(['nord dark']);
  });
});

describe('leaving the focused field', () => {
  it('leaves a text field or a select', async () => {
    for (const tagName of ['INPUT', 'textarea', 'SELECT']) {
      const blur = vi.fn();
      await commitFocusedField({ activeElement: { tagName, blur } });
      expect(blur).toHaveBeenCalledTimes(1);
    }
  });

  it('leaves anything else alone', async () => {
    const blur = vi.fn();
    await commitFocusedField({ activeElement: { tagName: 'BUTTON', blur } });
    await commitFocusedField({ activeElement: null });
    await commitFocusedField(undefined);
    expect(blur).not.toHaveBeenCalled();
  });
});

describe('the quit request', () => {
  it('sends what the window holds and answers every request', async () => {
    const write = vi.fn();
    const stop = pendingWrites.register(write);
    vi.mocked(listen).mockClear();
    vi.mocked(invoke).mockClear();
    await listenForQuitFlush();
    const [event, handler] = vi.mocked(listen).mock.calls[0];
    expect(event).toBe(FLUSH_REQUEST_EVENT);
    // The backend asks each window once a round, so a second quit round
    // gets a second answer.
    for (const round of [1, 2]) {
      (handler as EventCallback<unknown>)({ event, id: round, payload: round });
      await vi.waitFor(() => expect(invoke).toHaveBeenCalledTimes(round));
      expect(invoke).toHaveBeenLastCalledWith('pending_writes_flushed');
      expect(write).toHaveBeenCalledTimes(round);
    }
    stop();
  });
});
