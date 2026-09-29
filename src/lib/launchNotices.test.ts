import { beforeEach, describe, expect, it, vi } from 'vitest';

const fakes = vi.hoisted(() => ({
  invoke: vi.fn(),
  pushToast: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: fakes.invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));
vi.mock('./toasts', () => ({ pushToast: fakes.pushToast }));

const { launchNoticeLine, showLaunchNotices } = await import('./launchNotices');

const HEALER =
  'Vosh could not read the Healer profile file, so it will not save over it. Fix the file or switch to another profile.';

describe('launch notices', () => {
  beforeEach(() => {
    fakes.invoke.mockReset();
    fakes.pushToast.mockReset();
  });

  it('puts a notice on a yellow line of its own', () => {
    expect(launchNoticeLine(HEALER)).toBe(`\r\n\x1b[33m${HEALER}\x1b[0m\r\n`);
  });

  it('shows each notice in the terminal and as a toast', async () => {
    fakes.invoke.mockResolvedValueOnce([HEALER]);
    const written: string[] = [];
    await showLaunchNotices((text) => written.push(text));
    expect(fakes.invoke).toHaveBeenCalledWith('launch_notices_take');
    expect(written).toEqual([launchNoticeLine(HEALER)]);
    expect(fakes.pushToast).toHaveBeenCalledWith({ kind: 'error', message: HEALER });
  });

  it('shows nothing when launch kept nothing or the backend is away', async () => {
    const write = vi.fn();
    fakes.invoke.mockResolvedValueOnce([]);
    await showLaunchNotices(write);
    vi.spyOn(console, 'warn').mockImplementationOnce(() => undefined);
    fakes.invoke.mockRejectedValueOnce(new Error('no backend'));
    await showLaunchNotices(write);
    expect(write).not.toHaveBeenCalled();
    expect(fakes.pushToast).not.toHaveBeenCalled();
  });
});
