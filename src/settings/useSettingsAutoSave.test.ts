import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import uiFields from '../../fixtures/ui-config/fields.json';
import { setAffectsDisplay } from '../ipc/affects';
import { getUiConfig, setUiTheme, type RawUiConfig, type UiFields } from '../ipc/uiConfig';
import { pendingWrites } from '../lib/pendingWrites';
import { applyThemePrefs } from '../theme/theme';
import { queueSettingsChange } from './useSettingsAutoSave';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
// Applying a theme paints a page, and a test has none.
vi.mock('../theme/theme', async (actual) => ({
  ...(await actual<typeof import('../theme/theme')>()),
  applyThemePrefs: vi.fn(),
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
  const themeFields = [
    'theme',
    'follow_system_appearance',
    'light_theme',
    'dark_theme',
    'custom_themes',
  ];
  const themePrefsFields = themeFields.filter((field) => field !== 'custom_themes');

  it.each(Object.keys(values))(
    'saves %s alone and keeps what the main window picked',
    async (field) => {
      const backend = fakeBackend(opened);
      const copy = await getUiConfig();
      // The palette pick lands after Settings read its copy, and the
      // pane menu pick while the save waits.
      await setUiTheme('dracula');
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
