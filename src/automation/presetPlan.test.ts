import { beforeEach, describe, expect, it, vi } from 'vitest';
import { runPresetPlan } from './presetPlan';
import { defaultEnabledIds, presetById, PRESETS, presetTriggers } from './presets';
import {
  listMacros,
  listTriggers,
  presetsEnabledSet,
  presetsInstall,
  presetsRemove,
  type TriggerRecord,
} from '../ipc/automation';
import { presetEditsGet, presetEditsSet, type PresetEdits } from '../ipc/presetEdits';
import { getUiConfig, type UiConfig } from '../ipc/uiConfig';

vi.mock('../ipc/automation', async (actual) => ({
  ...(await actual<typeof import('../ipc/automation')>()),
  listTriggers: vi.fn(),
  listMacros: vi.fn(),
  presetsInstall: vi.fn(),
  presetsRemove: vi.fn(),
  presetsEnabledSet: vi.fn(),
}));
vi.mock('../ipc/presetEdits', () => ({ presetEditsGet: vi.fn(), presetEditsSet: vi.fn() }));
vi.mock('../ipc/uiConfig', () => ({ getUiConfig: vi.fn() }));

const DUAL = 'get 1.;dual 1.';
const WIELD = 'get 1.;wield 1.';

// Two profiles. Orla cleared Then send on disarm.secondary before the
// dual fix, and Maren left every preset alone.
const PROFILES: Record<string, { enabled: string[]; edits: PresetEdits }> = {
  Orla: {
    enabled: ['disarm_buff_fade'],
    edits: {
      disarm_buff_fade: {
        triggers: { 'disarm.secondary': { send: { value: '', was: WIELD } } },
      },
    },
  },
  Maren: { enabled: ['disarm_buff_fade'], edits: {} },
};

function profileOf(profile?: string | null) {
  return PROFILES[profile ?? 'Orla'];
}

function installedFor(profile: string): TriggerRecord[] {
  const call = vi.mocked(presetsInstall).mock.calls.find((c) => c[2] === profile);
  return call?.[0] ?? [];
}

function sendOf(triggers: TriggerRecord[], name: string): string | undefined {
  const send = triggers.find((t) => t.name === name)?.actions.find((a) => a.kind === 'send');
  return send?.kind === 'send' ? send.template : undefined;
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(getUiConfig).mockImplementation(
    async (profile) => ({ enabled_presets: profileOf(profile).enabled }) as UiConfig,
  );
  vi.mocked(presetEditsGet).mockImplementation(async (profile) => profileOf(profile).edits);
  vi.mocked(listTriggers).mockResolvedValue([]);
  vi.mocked(listMacros).mockResolvedValue([]);
  vi.mocked(presetsInstall).mockResolvedValue({ installed: 0, removed: [] });
  vi.mocked(presetsEnabledSet).mockResolvedValue({ installed: 0, removed: [] });
});

describe('runPresetPlan', () => {
  it('installs each preset that is on as it ships where you made no edit', async () => {
    vi.mocked(getUiConfig).mockResolvedValue({ enabled_presets: [] as string[] } as UiConfig);
    vi.mocked(presetEditsGet).mockResolvedValue({});
    const notice = await runPresetPlan();
    const on = PRESETS.filter((p) => defaultEnabledIds().includes(p.id));
    expect(presetsInstall).toHaveBeenCalledWith(
      on.flatMap((p) => presetTriggers(p)),
      [],
      null,
    );
    expect(presetEditsSet).not.toHaveBeenCalled();
    expect(notice).toEqual({ told: [], removed: [] });
  });

  it('lays the edits of the profile that opens over its presets', async () => {
    const notice = await runPresetPlan('Orla');
    expect(getUiConfig).toHaveBeenCalledWith('Orla');
    expect(presetEditsGet).toHaveBeenCalledWith('Orla');
    expect(listTriggers).toHaveBeenCalledWith('Orla');
    expect(sendOf(installedFor('Orla'), 'disarm.secondary')).toBeUndefined();
    expect(sendOf(installedFor('Orla'), 'disarm.primary')).toBe(WIELD);
    expect(notice.told).toEqual([
      { preset: 'disarm_buff_fade', trigger: 'disarm.secondary', row: 'send' },
    ]);
    // The flag is saved as seen, so the next launch tells nothing.
    expect(presetEditsSet).toHaveBeenCalledWith(
      'disarm_buff_fade',
      { triggers: { 'disarm.secondary': { send: { value: '', was: DUAL, seen: DUAL } } } },
      'Orla',
    );
  });

  it('runs again on a switch with the other profile’s edits', async () => {
    await runPresetPlan('Orla');
    const notice = await runPresetPlan('Maren');
    expect(presetEditsGet).toHaveBeenLastCalledWith('Maren');
    expect(sendOf(installedFor('Maren'), 'disarm.secondary')).toBe(DUAL);
    expect(installedFor('Maren')).toEqual(presetTriggers(presetById('disarm_buff_fade')!));
    expect(notice).toEqual({ told: [], removed: [] });
    expect(presetEditsSet).toHaveBeenCalledTimes(1);
  });

  it('takes out a preset that is off and runs one plan at a time', async () => {
    vi.mocked(listTriggers).mockResolvedValue([
      { preset: 'healing_basics' } as TriggerRecord,
      { preset: 'disarm_buff_fade' } as TriggerRecord,
    ]);
    const order: string[] = [];
    vi.mocked(presetsInstall).mockImplementation(async (_t, _m, profile) => {
      order.push(`install ${profile}`);
      return { installed: 0, removed: [] };
    });
    vi.mocked(getUiConfig).mockImplementation(async (profile) => {
      order.push(`read ${profile}`);
      return { enabled_presets: profileOf(profile).enabled } as UiConfig;
    });
    await Promise.all([runPresetPlan('Orla'), runPresetPlan('Maren')]);
    expect(presetsRemove).toHaveBeenCalledWith('healing_basics', 'Orla');
    expect(presetsRemove).toHaveBeenCalledWith('healing_basics', 'Maren');
    expect(order).toEqual(['read Orla', 'install Orla', 'read Maren', 'install Maren']);
  });

  it('turns presets on and off through presets_enabled_set', async () => {
    const switches = [{ id: 'terror_events', on: true }];
    await runPresetPlan('Maren', switches);
    const built = ['disarm_buff_fade', 'terror_events'].flatMap((id) =>
      presetTriggers(presetById(id)!),
    );
    expect(presetsEnabledSet).toHaveBeenCalledWith(switches, built, [], 'Maren');
    expect(presetsInstall).not.toHaveBeenCalled();
  });

  it('saves no seen when the install fails', async () => {
    vi.mocked(presetsInstall).mockRejectedValue(new Error('no such profile'));
    vi.spyOn(console, 'error').mockImplementation(() => {});
    expect(await runPresetPlan('Orla')).toEqual({ told: [], removed: [] });
    expect(presetEditsSet).not.toHaveBeenCalled();
  });
});
