import { afterEach, describe, expect, it, vi } from 'vitest';

// The toast a saved scene raises in the main window.

const calls: { cmd: string; args: unknown }[] = [];
vi.mock('@tauri-apps/api/core', () => ({
  invoke: async (cmd: string, args: unknown) => {
    calls.push({ cmd, args });
    return null;
  },
}));

vi.stubGlobal('window', { setTimeout, clearTimeout });

afterEach(async () => {
  const { dismissToast, getToasts } = await import('../stores/toasts');
  for (const toast of getToasts()) dismissToast(toast.id);
  calls.length = 0;
});

describe('the saved scene toast', () => {
  it('names the file and shows it in the file manager of each platform', async () => {
    const { sceneSavedToast } = await import('./useSceneSaved');
    const { getToasts } = await import('../stores/toasts');
    sceneSavedToast('Thickening Woods, October 3.html', 'macos');
    const [toast] = getToasts();
    expect(toast).toMatchObject({
      kind: 'success',
      message: 'Saved the scene',
      meta: 'Thickening Woods, October 3.html',
    });
    expect(toast.action?.label).toBe('Show in Finder');
    toast.action?.run();
    await Promise.resolve();
    expect(calls).toEqual([
      { cmd: 'scene_reveal', args: { name: 'Thickening Woods, October 3.html' } },
    ]);
    sceneSavedToast('Thickening Woods, October 3 (2).html', 'windows');
    expect(getToasts().at(-1)?.action?.label).toBe('Show in Explorer');
  });
});
