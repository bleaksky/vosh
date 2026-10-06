import { beforeEach, describe, expect, it, vi } from 'vitest';

const tauri = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn((name: string, handler: (event: { payload: unknown }) => void) => {
    tauri.handlers.set(name, handler);
    return Promise.resolve(() => tauri.handlers.delete(name));
  }),
}));

import { followCardProfile } from './promptCardSync';

const fire = (name: string, payload: unknown = null) => tauri.handlers.get(name)?.({ payload });

describe('the card follows the profile it saves for', () => {
  beforeEach(() => tauri.handlers.clear());

  it('opens again for the profile you switch to, or a config that replaced it', async () => {
    const reopen = vi.fn();
    const identity = vi.fn();
    const stop = await followCardProfile({ reopen, identity });
    fire('vosh://ui-config-replaced');
    fire('vosh://profile-switched', 'Healer');
    expect(reopen).toHaveBeenCalledTimes(2);
    expect(identity).not.toHaveBeenCalled();
    stop();
    expect(tauri.handlers.size).toBe(0);
  });

  it('names who it saves for again when you log in', async () => {
    const reopen = vi.fn();
    const identity = vi.fn();
    await followCardProfile({ reopen, identity });
    const who = { host: 'h', port: 1, character: 'Tester', profile: 'default' };
    fire('vosh://session-identity-changed', who);
    expect(identity).toHaveBeenCalledWith(who);
    expect(reopen).not.toHaveBeenCalled();
  });
});
