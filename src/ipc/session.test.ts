import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import gmcpEvents from '../../fixtures/ipc/gmcp-events.json';
import { aabahranPacket } from '../test/aabahranGmcp';
import { onGmcpPackage, sendInput, sendMaskedInput, stopWalk } from './session';

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
    await stopWalk();
    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('session_walk_stop');
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
