import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import gmcpEvents from '../../fixtures/ipc/gmcp-events.json';
import { aabahranPacket } from '../test/aabahranGmcp';
import {
  onGmcpPackage,
  onScreenReader,
  reconnectCancel,
  reconnectGet,
  reconnectNow,
  reconnectSet,
  sendInput,
  sendMaskedInput,
  stopWalk,
  walkRoute,
} from './session';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

describe('sending a line', () => {
  // Made up value only. It is nobody's password.
  const SECRET = 'Tr0ub4dor&3';

  afterEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it('sends a line from the masked field through the masked send only', async () => {
    vi.mocked(invoke).mockClear();
    await sendMaskedInput(SECRET);
    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('session_send_masked', { line: SECRET });
  });

  it('runs a typed command through the input pipeline', async () => {
    vi.mocked(invoke).mockClear();
    await sendInput('look');
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('session_send_input', { line: 'look' });
  });

  it('stops a walk on Esc with a call of its own, which sends the game nothing', async () => {
    vi.mocked(invoke).mockClear();
    await stopWalk(2);
    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('session_walk_stop', { session: 2 });
  });

  it('walks a path clicked on the map with the rooms each step should reach', async () => {
    vi.mocked(invoke).mockClear();
    await walkRoute('2w', 4406, [4405, 4404], 2);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('session_walk_route', {
      steps: '2w',
      start: 4406,
      rooms: [4405, 4404],
      session: 2,
    });
  });

  it('names the session the line was typed in', async () => {
    vi.mocked(invoke).mockClear();
    await sendInput('look', 2);
    await sendMaskedInput(SECRET, 2);
    expect(vi.mocked(invoke).mock.calls).toEqual([
      ['session_send_input', { line: 'look', session: 2 }],
      ['session_send_masked', { line: SECRET, session: 2 }],
    ]);
  });
});

describe('the redial after a drop', () => {
  it('names the session whose redial the notice acts on, and the profile of the switch', async () => {
    vi.mocked(invoke).mockClear();
    await reconnectNow(2);
    await reconnectCancel(2);
    await reconnectGet();
    await reconnectSet(false, 'Orla');
    expect(vi.mocked(invoke).mock.calls).toEqual([
      ['session_reconnect_now', { session: 2 }],
      ['session_reconnect_cancel', { session: 2 }],
      ['reconnect_get', { profile: undefined }],
      ['reconnect_set', { on: false, profile: 'Orla' }],
    ]);
  });
});

// The session sends each GMCP package on the event this shared file
// names for it. The fake MUD test in src-tauri reads the same file, so a
// change to the encoding on one side alone fails one of the two.
describe('onGmcpPackage', () => {
  it('listens on the event the session sends each package on', async () => {
    for (const { package: name, event } of gmcpEvents.cases) {
      vi.mocked(listen).mockClear();
      await onGmcpPackage(name, () => {});
      expect(vi.mocked(listen)).toHaveBeenCalledWith(event, expect.any(Function));
    }
  });

  it('hands its listener the data the session sends and its id', async () => {
    const { package: name, data } = aabahranPacket('char-vitals.gmcp');
    vi.mocked(listen).mockClear();
    const heard = vi.fn();
    await onGmcpPackage(name, heard);
    const [event, handler] = vi.mocked(listen).mock.calls[0];
    handler({ event, id: 1, payload: { session: 2, data } });
    expect(heard).toHaveBeenCalledWith(data, 2);
  });
});

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
