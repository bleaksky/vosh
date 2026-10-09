import { emitSettingsGotoTab, openSettingsWindow } from '../ipc/windows';

// Opening Settings from the main window. The Settings window may not
// exist yet, so a target travels twice: through localStorage for a
// cold open and through an event for a window that is already up.
// SettingsWindow reads both and resolves the string with
// resolveSettingsTarget (src/lib/settingsNav.ts).

/** Where a cold open finds its target. */
export const SETTINGS_PENDING_KEY = 'vosh.settings.pendingTab';

/** Open Settings on `target`, a deep link like `automation:macros` or
 *  `characters:Ilsabet#tracked`. */
export function openSettingsTab(target: string): void {
  try {
    localStorage.setItem(SETTINGS_PENDING_KEY, target);
  } catch {
    // Storage unavailable. The event still reaches an open window.
  }
  void emitSettingsGotoTab(target);
  openSettingsWindow();
}
