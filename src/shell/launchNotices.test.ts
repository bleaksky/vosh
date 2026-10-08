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
vi.mock('../stores/toasts', () => ({ pushToast: fakes.pushToast }));

const { launchNoticeLine, MIGRATION_APPLIED_NOTICE, showLaunchNotices, showMigrationApplied } =
  await import('./launchNotices');

const HEALER =
  'Vosh could not read the Healer profile file, so it will not save over it. Fix the file or switch to another profile.';

const VOICEOVER =
  'VoiceOver is on. To hear the game, turn on Read new game lines in Settings under Accessibility.';

describe('launch notices', () => {
  beforeEach(() => {
    fakes.invoke.mockReset();
    fakes.pushToast.mockReset();
  });

  it('puts a notice on a yellow line of its own', () => {
    expect(launchNoticeLine(HEALER)).toBe(`\r\n\x1b[33m${HEALER}\x1b[0m\r\n`);
  });

  it('shows each notice in the terminal and as a toast', async () => {
    fakes.invoke.mockResolvedValueOnce([{ kind: 'error', message: HEALER }]);
    const written: string[] = [];
    await showLaunchNotices((text) => written.push(text));
    expect(fakes.invoke).toHaveBeenCalledWith('launch_notices_take');
    expect(written).toEqual([launchNoticeLine(HEALER)]);
    expect(fakes.pushToast).toHaveBeenCalledWith({ kind: 'error', message: HEALER });
  });

  it('shows the screen reader pointer as info, not as an error', async () => {
    fakes.invoke.mockResolvedValueOnce([{ kind: 'info', message: VOICEOVER }]);
    const written: string[] = [];
    await showLaunchNotices((text) => written.push(text));
    expect(written).toEqual([launchNoticeLine(VOICEOVER)]);
    expect(fakes.pushToast).toHaveBeenCalledWith({ kind: 'info', message: VOICEOVER });
    expect(fakes.pushToast).not.toHaveBeenCalledWith(expect.objectContaining({ kind: 'error' }));
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

describe('the notice after the move to loadouts', () => {
  beforeEach(() => {
    fakes.pushToast.mockReset();
  });

  it('says Vosh saves nothing you change before you quit', () => {
    expect(MIGRATION_APPLIED_NOTICE).toBe(
      'The move to loadouts is done. Vosh does not save the changes you make before you quit, so quit Vosh and open it again now.',
    );
    expect(MIGRATION_APPLIED_NOTICE).not.toMatch(/[:;‒-―]/);
  });

  it('shows it in the terminal and in a toast that stays until you close it', () => {
    const written: string[] = [];
    showMigrationApplied((text) => written.push(text));
    expect(written).toEqual([launchNoticeLine(MIGRATION_APPLIED_NOTICE)]);
    expect(fakes.pushToast).toHaveBeenCalledWith({
      kind: 'info',
      message: MIGRATION_APPLIED_NOTICE,
      sticky: true,
    });
  });
});
