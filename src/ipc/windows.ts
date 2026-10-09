// The windows around the page and the app that holds them. Open Settings
// and Help and steer them, hear the menu bar and keep it current, tell a
// new window what it opens on, answer the quit request, take the launch
// notices and quit. A call or subscribe returns the Tauri promise as it
// is, so each caller keeps its own catch and its own checks.

import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { MenuState } from '../lib/appMenu';
import type { Appearance } from '../theme/chrome';
import {
  APP_MENU,
  FLUSH_PENDING_WRITES,
  HELP_FIND,
  HELP_GOTO,
  HELP_OPEN,
  SETTINGS_FIND,
  SETTINGS_GOTO_TAB,
} from './events';

/** One thing launch has to tell you. An error says something went wrong,
 *  such as a profile file Vosh could not read and will not save over, and
 *  info only points you somewhere, such as the screen reader setting. */
export interface LaunchNotice {
  kind: 'error' | 'info';
  message: string;
}

/** The notices launch kept for you. The first call takes them, and every
 *  later call gets none. */
export async function launchNoticesTake(): Promise<LaunchNotice[]> {
  return invoke('launch_notices_take');
}

// Cleanly quit Vosh. The post-migration prompt uses this so the
// user can relaunch into Path B mode in one click.
export async function appQuit(): Promise<void> {
  return invoke('app_quit');
}

/** Open Settings, or focus it, where it is. */
export function openSettingsWindow(): void {
  invoke('open_settings_window').catch((e: unknown) => {
    console.error('[settings] open_settings_window failed', e);
  });
}

/** Take an open Settings window to `target`, a deep link that
 *  resolveSettingsTarget reads. */
export function emitSettingsGotoTab(target: string): Promise<void> {
  return emit(SETTINGS_GOTO_TAB, target);
}

/** Hear a window send Settings to a target. */
export function subscribeSettingsGotoTab(cb: (target: string) => void): Promise<UnlistenFn> {
  return listen<string>(SETTINGS_GOTO_TAB, (event) => cb(event.payload));
}

/** Hear Find in the menu bar, chosen while Settings is in front. */
export function subscribeSettingsFind(cb: () => void): Promise<UnlistenFn> {
  return listen(SETTINGS_FIND, () => cb());
}

/** Open Help, or bring it forward, where it is. */
export function openHelpWindow(): void {
  invoke('open_help_window').catch((e: unknown) => {
    console.error('[help] open_help_window failed', e);
  });
}

/** Take an open Help window to `target`, which resolveHelpTarget reads. */
export function emitHelpGoto(target: string): Promise<void> {
  return emit(HELP_GOTO, target);
}

/** Hear a window send Help to a target. */
export function subscribeHelpGoto(cb: (target: string) => void): Promise<UnlistenFn> {
  return listen<string>(HELP_GOTO, (event) => cb(event.payload));
}

/** Hear Find in the menu bar, chosen while Help is in front. */
export function subscribeHelpFind(cb: () => void): Promise<UnlistenFn> {
  return listen(HELP_FIND, () => cb());
}

/** Hear `#help` from the command line, with the words after it. */
export function subscribeHelpOpen(cb: (words: string) => void): Promise<UnlistenFn> {
  return listen<string>(HELP_OPEN, (event) => cb(event.payload));
}

/** Send the menu bar a snapshot of the main window. */
export function menuSetState(state: MenuState): Promise<void> {
  return invoke('menu_set_state', { state });
}

/** Edit, then Copy, with the native grid. With `terminal` a grid
 *  selection wins. Otherwise the system copies the page's own. */
export function menuCopy(terminal: boolean): void {
  invoke('menu_copy', { terminal }).catch((e: unknown) => {
    console.error('[menu] menu_copy failed', e);
  });
}

/** Hear menu commands, each by its palette entry id. Main window only. */
export function subscribeAppMenu(cb: (id: unknown) => void): Promise<UnlistenFn> {
  return listen<unknown>(APP_MENU, (event) => cb(event.payload));
}

/** What a new window opens on: the theme's ground as `#rrggbb`, null
 *  when it is not one solid color, and the native appearance, null to
 *  follow the system. */
export function windowBackdropSet(backdrop: {
  background: string | null;
  appearance: Appearance | null;
}): Promise<void> {
  return invoke('window_backdrop_set', { ...backdrop });
}

/** Hear the backend ask, on quit, for the writes this window holds. */
export function subscribeFlushPendingWrites(cb: () => void): Promise<UnlistenFn> {
  return listen<unknown>(FLUSH_PENDING_WRITES, () => cb());
}

/** The answer to the quit request, once this window has sent what it
 *  held. */
export function pendingWritesFlushed(): Promise<void> {
  return invoke('pending_writes_flushed');
}
