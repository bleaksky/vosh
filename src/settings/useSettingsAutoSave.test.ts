import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import uiFields from '../../fixtures/ui-config/fields.json';
import { AFFECTS_DISPLAY_FIELDS, setAffectsDisplay } from '../ipc/affects';
import { THEME_PREFS_FIELDS } from '../ipc/theme';
import {
  getUiConfig,
  normalizeUiConfig,
  setUiTheme,
  type RawUiConfig,
  type UiFields,
} from '../ipc/uiConfig';
import { pendingWrites } from '../lib/pendingWrites';
import { applyThemePrefs, getThemePrefs, themePrefsOf } from '../theme/theme';
import { queueSettingsChange, settingsSaveHolds } from './useSettingsAutoSave';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
// Applying a theme paints a page, and a test has none. So the test
// says which theme fields the window shows.
vi.mock('../theme/theme', async (actual) => ({
  ...(await actual<typeof import('../theme/theme')>()),
  applyThemePrefs: vi.fn(),
  getThemePrefs: vi.fn(() => null),
}));

const opened: RawUiConfig = {
  theme: 'gruvbox',
  auto_update: false,
  font_family: 'Menlo',
  font_size: 14,
  tracked_affects: [],
  enabled_presets: [],
};

/** One field setter writes, as ui_set_fields sends it. */
interface FieldValue {
  field: string;
  value: unknown;
}

/** A backend that keeps one profile's UI config and lets each setter
 *  write only what it names, as ui_set_fields, ui_set_theme and
 *  ui_set_affects_display do. */
function fakeBackend(initial: RawUiConfig) {
  const stored: Record<string, unknown> = { ...initial };
  const fieldSaves: FieldValue[][] = [];
  vi.mocked(invoke).mockImplementation(((command: string, args?: Record<string, unknown>) => {
    if (command === 'ui_get_config') return Promise.resolve({ ...stored });
    if (command === 'ui_set_fields') {
      const fields = args?.fields as FieldValue[];
      fieldSaves.push(fields);
      for (const { field, value } of fields) stored[field] = value;
    }
    if (command === 'ui_set_theme') stored.theme = args?.theme;
    if (command === 'ui_set_affects_display' && args?.style != null) {
      stored.affects_style = args.style;
    }
    return Promise.resolve();
  }) as typeof invoke);
  return { stored, fieldSaves };
}

// The plan's two window test, one case per setting. Settings changes one
// field while the main window picks a theme in the palette and a style
// in the Affects pane menu.
describe('a Settings change', () => {
  const values: Record<string, unknown> = uiFields.fields;
  const themePrefsFields: readonly string[] = THEME_PREFS_FIELDS;
  const themeFields = [...themePrefsFields, 'custom_themes'];

  it.each(Object.keys(values))(
    'saves %s alone and keeps what the main window picked',
    async (field) => {
      const backend = fakeBackend(opened);
      const copy = await getUiConfig();
      // The palette pick lands after Settings read its copy, and Settings
      // shows it. The pane menu pick lands while the save waits.
      await setUiTheme('dracula');
      vi.mocked(getThemePrefs).mockReturnValue({ ...themePrefsOf(copy), theme: 'dracula' });
      const saved = vi.fn();
      const failed = vi.fn();
      queueSettingsChange(copy, { [field]: values[field] } as UiFields, 250, { saved, failed });
      await setAffectsDisplay({ style: 'countdown' });
      vi.mocked(emit).mockClear();
      vi.mocked(applyThemePrefs).mockClear();
      expect(await pendingWrites.flushAll()).toBe(true);

      expect(failed).not.toHaveBeenCalled();
      expect(saved).toHaveBeenCalledTimes(1);
      expect(backend.fieldSaves).toEqual([[{ field, value: values[field] }]]);
      expect(backend.stored).toMatchObject({
        theme: field === 'theme' ? values.theme : 'dracula',
        affects_style: field === 'affects_style' ? values.affects_style : 'countdown',
        [field]: values[field],
      });
      // The copy's older theme and style reach no window, and only a
      // theme of your own shows on Settings.
      const events = vi.mocked(emit).mock.calls.map(([event]) => event);
      if (!themePrefsFields.includes(field)) {
        expect(events).not.toContain('vosh://theme-prefs-changed');
        expect(events).not.toContain('vosh://theme-changed');
      }
      if (!field.startsWith('affects_')) {
        expect(events).not.toContain('vosh://affects-display-changed');
      }
      expect(applyThemePrefs).toHaveBeenCalledTimes(themeFields.includes(field) ? 1 : 0);
      // A theme field you set shows as you set it. A custom themes save
      // keeps the palette pick Settings shows.
      if (themePrefsFields.includes(field)) {
        expect(applyThemePrefs).toHaveBeenCalledWith(
          expect.objectContaining({ [field]: values[field] }),
        );
      }
      if (field === 'custom_themes') {
        expect(applyThemePrefs).toHaveBeenCalledWith(expect.objectContaining({ theme: 'dracula' }));
      }
    },
  );

  it('merges the edits you make while the save waits', async () => {
    const backend = fakeBackend(opened);
    const copy = await getUiConfig();
    const report = { saved: vi.fn(), failed: vi.fn() };
    // React can run a state updater twice, with the same copy.
    const larger = queueSettingsChange(copy, { font_size: 15 }, 250, report);
    queueSettingsChange(copy, { font_size: 15 }, 250, report);
    const back = queueSettingsChange(larger, { font_size: 14 }, 250, report);
    queueSettingsChange(back, { tick_count: 'down' }, 250, report);
    vi.mocked(emit).mockClear();
    await pendingWrites.flushAll();

    expect(backend.fieldSaves).toEqual([
      [
        { field: 'font_size', value: 14 },
        { field: 'tick_count', value: 'down' },
      ],
    ]);
    // The font size ends where it started, so no window hears of it.
    expect(vi.mocked(emit).mock.calls).toEqual([['vosh://tick-count-changed', 'down']]);
    expect(report.saved).toHaveBeenCalledTimes(1);
  });
});

// Settings hears its own theme and affects display broadcasts, and the
// window leaves a field alone while a save of its own holds it.
describe('what a Settings save holds', () => {
  const copy = normalizeUiConfig(opened);

  /** Hold the next ui_set_fields until the test answers it. */
  function answerLater(): (outcome: 'saved' | 'failed') => void {
    let settle: (outcome: 'saved' | 'failed') => void = () => {};
    vi.mocked(invoke).mockImplementationOnce(
      (() =>
        new Promise<void>((resolve, reject) => {
          settle = (outcome) =>
            outcome === 'saved' ? resolve() : reject(new Error('the profile is gone'));
        })) as typeof invoke,
    );
    return (outcome) => settle(outcome);
  }

  it('holds the theme fields while a theme pick waits and while it sends', async () => {
    const report = { saved: vi.fn(), failed: vi.fn() };
    queueSettingsChange(copy, { theme: 'dracula' }, 250, report);
    expect(settingsSaveHolds(THEME_PREFS_FIELDS)).toBe(true);
    expect(settingsSaveHolds(AFFECTS_DISPLAY_FIELDS)).toBe(false);
    const answer = answerLater();
    const landed = pendingWrites.flushAll();
    expect(settingsSaveHolds(THEME_PREFS_FIELDS)).toBe(true);
    answer('saved');
    await landed;

    expect(report.saved).toHaveBeenCalledTimes(1);
    expect(settingsSaveHolds(THEME_PREFS_FIELDS)).toBe(false);
  });

  it('holds the theme until the last of two picks lands', async () => {
    const report = { saved: vi.fn(), failed: vi.fn() };
    const answerFirst = answerLater();
    const answerSecond = answerLater();
    const first = queueSettingsChange(copy, { theme: 'dracula' }, 250, report);
    const firstLanded = pendingWrites.flushAll();
    queueSettingsChange(first, { theme: 'gruvbox' }, 250, report);
    const secondLanded = pendingWrites.flushAll();
    // The first pick lands and its broadcast comes back older than the
    // second pick.
    answerFirst('saved');
    await firstLanded;
    expect(settingsSaveHolds(THEME_PREFS_FIELDS)).toBe(true);
    answerSecond('saved');
    await secondLanded;

    expect(report.saved).toHaveBeenCalledTimes(2);
    expect(settingsSaveHolds(THEME_PREFS_FIELDS)).toBe(false);
  });

  it('lets go of the fields a failed save held', async () => {
    const report = { saved: vi.fn(), failed: vi.fn() };
    queueSettingsChange(copy, { affects_tint: true }, 250, report);
    const answer = answerLater();
    const landed = pendingWrites.flushAll();
    expect(settingsSaveHolds(AFFECTS_DISPLAY_FIELDS)).toBe(true);
    answer('failed');
    await landed;

    expect(report.failed).toHaveBeenCalledTimes(1);
    expect(settingsSaveHolds(AFFECTS_DISPLAY_FIELDS)).toBe(false);
  });
});

// Each save names the profile Settings showed as you made it. Tolliver
// plays Default and Orla plays Build, with Orla's session selected in
// the main window.
describe('a Settings save and its profile', () => {
  const copy = normalizeUiConfig(opened);
  const ROWS = [
    {
      id: 1,
      name: null,
      character: 'Tolliver',
      host: 'play.theforsakenlands.com',
      port: 1848,
      tls: false,
      profile: 'default',
      connected: true,
      selected: false,
    },
    {
      id: 2,
      name: null,
      character: 'Orla',
      host: 'play.theforsakenlands.com',
      port: 1825,
      tls: false,
      profile: 'Build',
      connected: true,
      selected: true,
    },
  ];

  /** What each ui_set_fields wrote, and to which profile. */
  async function sessionsBackend() {
    const saves: [unknown, unknown][] = [];
    vi.mocked(invoke).mockImplementation(((command: string, args?: Record<string, unknown>) => {
      if (command === 'sessions_list') return Promise.resolve(ROWS);
      if (command === 'ui_set_fields') saves.push([args?.profile, args?.fields]);
      return Promise.resolve();
    }) as typeof invoke);
    const { startSessionsStore } = await import('../stores/session/sessionsStore');
    startSessionsStore();
    await new Promise((resolve) => setTimeout(resolve, 0));
    return saves;
  }

  it('lands a waiting save on its own profile, apart from a later edit on another', async () => {
    const saves = await sessionsBackend();
    const report = { saved: vi.fn(), failed: vi.fn() };
    // You change Default's font size, and Settings follows Orla to Build
    // before the save goes.
    queueSettingsChange(copy, { font_size: 15 }, 250, report, 'default');
    queueSettingsChange(copy, { tick_count: 'down' }, 250, report, 'Build');
    vi.mocked(emit).mockClear();
    await pendingWrites.flushAll();

    expect(saves).toEqual([
      ['default', [{ field: 'font_size', value: 15 }]],
      ['Build', [{ field: 'tick_count', value: 'down' }]],
    ]);
    expect(report.saved).toHaveBeenCalledTimes(2);
    // The main window shows Build, so only Build's save reaches it.
    expect(vi.mocked(emit).mock.calls).toEqual([['vosh://tick-count-changed', 'down']]);
  });

  it('drops a waiting save whose profile no session plays any more', async () => {
    const saves = await sessionsBackend();
    const report = { saved: vi.fn(), failed: vi.fn() };
    queueSettingsChange(copy, { theme: 'dracula' }, 250, report, 'Healer');
    expect(settingsSaveHolds(THEME_PREFS_FIELDS)).toBe(true);
    await pendingWrites.flushAll();

    expect(saves).toEqual([]);
    expect(report.saved).not.toHaveBeenCalled();
    expect(report.failed).not.toHaveBeenCalled();
    expect(settingsSaveHolds(THEME_PREFS_FIELDS)).toBe(false);
  });
});
