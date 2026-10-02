import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';

// Opening Settings from the main window. The Settings window may not
// exist yet, so a target travels twice: through localStorage for a
// cold open and through an event for a window that is already up.
// SettingsApp reads both and resolves the string with
// resolveSettingsTarget (src/lib/settingsNav.ts).

/** Where a cold open finds its target. */
export const SETTINGS_PENDING_KEY = 'vosh.settings.pendingTab';
/** The event an open Settings window listens on. */
export const SETTINGS_GOTO_EVENT = 'vosh://settings-goto-tab';

/** Open Settings, or focus it, where it is. */
export function openSettingsWindow(): void {
  invoke('open_settings_window').catch((e: unknown) => {
    console.error('[settings] open_settings_window failed', e);
  });
}

/** Open Settings on `target`, a deep link like `automation:macros`,
 *  `characters:Ilsabet#tracked`, or an old tab id like `themes`. */
export function openSettingsTab(target: string): void {
  try {
    localStorage.setItem(SETTINGS_PENDING_KEY, target);
  } catch {
    // Storage unavailable. The event still reaches an open window.
  }
  void emit(SETTINGS_GOTO_EVENT, target);
  openSettingsWindow();
}
