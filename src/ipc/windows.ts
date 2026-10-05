// Launch notices and quitting Vosh.

import { invoke } from '@tauri-apps/api/core';

/** The sentences launch kept for you, such as a profile file Vosh could
 *  not read and will not save over. The first call takes them, and every
 *  later call gets none. */
export async function launchNoticesTake(): Promise<string[]> {
  return invoke('launch_notices_take');
}

// Cleanly quit Vosh. The post-migration prompt uses this so the
// user can relaunch into Path B mode in one click.
export async function appQuit(): Promise<void> {
  return invoke('app_quit');
}
