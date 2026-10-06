import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { listen, type EventCallback } from '@tauri-apps/api/event';
import { migrationAnalyze, migrationApply, subscribeMigrationApplied } from './wizard';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

describe('the shared catalog wizard calls', () => {
  // The backend knows the preset library only from these calls. A preset
  // the library no longer has stays off for characters whose file lacks
  // it.
  it('send the preset library with the preview and the apply', async () => {
    vi.mocked(invoke).mockClear();
    await migrationAnalyze(['healing_basics', 'herb_labels']);
    expect(invoke).toHaveBeenCalledWith('migration_analyze', {
      library: ['healing_basics', 'herb_labels'],
    });
    await migrationApply([], ['healing_basics']);
    expect(invoke).toHaveBeenCalledWith('migration_apply', {
      resolutions: [],
      library: ['healing_basics'],
    });
  });

  it('hear when the move to loadouts is done', async () => {
    const heard = vi.fn();
    vi.mocked(listen).mockClear();
    await subscribeMigrationApplied(heard);
    const [event, handler] = vi.mocked(listen).mock.calls[0];
    expect(event).toBe('vosh://migration-applied');
    (handler as EventCallback<unknown>)({ event, id: 1, payload: null });
    expect(heard).toHaveBeenCalledTimes(1);
  });
});
