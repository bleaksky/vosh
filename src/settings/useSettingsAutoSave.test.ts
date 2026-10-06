import { act, createElement } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type EventCallback } from '@tauri-apps/api/event';
import uiFields from '../../fixtures/ui-config/fields.json';
import { AFFECTS_DISPLAY_FIELDS, setAffectsDisplay } from '../ipc/affects';
import { UI_CONFIG_REPLACED } from '../ipc/events';
import { THEME_PREFS_FIELDS } from '../ipc/theme';
import {
  getUiConfig,
  normalizeUiConfig,
  setUiTheme,
  type RawUiConfig,
  type UiFields,
} from '../ipc/uiConfig';
import { pendingWrites } from '../lib/pendingWrites';
import { FakeDocument, FakeElement, FakeNode } from '../test/fakeDom';
import { applyThemePrefs } from '../theme/theme';
import { queueSettingsChange, settingsSaveHolds, useSettingsAutoSave } from './useSettingsAutoSave';

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
  const themePrefsFields: readonly string[] = THEME_PREFS_FIELDS;
  const themeFields = [...themePrefsFields, 'custom_themes'];

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

  // The window drops the save waiting when the backend replaces the
  // config, so this mounts the hook that hears the replace.
  describe('on a replaced config', () => {
    const doc = new FakeDocument();
    let createRoot: typeof import('react-dom/client').createRoot;

    beforeAll(async () => {
      vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
      vi.stubGlobal('document', doc);
      vi.stubGlobal('window', {
        document: doc,
        location: { protocol: 'about:' },
        HTMLIFrameElement: class {},
        addEventListener() {},
        removeEventListener() {},
      });
      vi.stubGlobal('navigator', { userAgent: 'node', platform: '' });
      vi.stubGlobal('Node', FakeNode);
      vi.stubGlobal('Element', FakeElement);
      vi.stubGlobal('HTMLElement', FakeElement);
      // React DOM checks for a DOM once, when it loads.
      ({ createRoot } = await import('react-dom/client'));
    });

    afterAll(() => {
      vi.unstubAllGlobals();
    });

    function Saver() {
      useSettingsAutoSave(
        () => {},
        () => {},
      );
      return null;
    }

    it('lets go of the fields the dropped save held', async () => {
      let replace = () => {};
      vi.mocked(listen).mockImplementationOnce((event, handler) => {
        if (event === UI_CONFIG_REPLACED) {
          replace = () => (handler as EventCallback<unknown>)({ event, id: 0, payload: null });
        }
        return Promise.resolve(() => {});
      });
      const root = createRoot(doc.createElement('div') as unknown as HTMLElement);
      await act(async () => root.render(createElement(Saver)));
      vi.mocked(invoke).mockClear();
      const report = { saved: vi.fn(), failed: vi.fn() };
      queueSettingsChange(copy, { theme: 'dracula' }, 250, report);
      expect(settingsSaveHolds(THEME_PREFS_FIELDS)).toBe(true);
      replace();
      expect(settingsSaveHolds(THEME_PREFS_FIELDS)).toBe(false);
      await act(async () => root.unmount());

      expect(invoke).not.toHaveBeenCalledWith('ui_set_fields', expect.anything());
      expect(report.saved).not.toHaveBeenCalled();
    });
  });
});
