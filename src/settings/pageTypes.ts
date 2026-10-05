import type { UiConfig } from '../ipc/uiConfig';
import type { SettingsTarget } from '../lib/settingsNav';

/** Replace the window's UiConfig copy. A page edits through this and
 *  never keeps its own copy of the config, so every page shows your
 *  latest edit and each save knows what a field held before it. */
export type SetUiConfig = (updater: (prev: UiConfig | null) => UiConfig | null) => void;

/** Asked before the frame leaves a page for another group. Return true
 *  to hold the navigation, and call `proceed` later to go on, after
 *  the page asks about unsaved changes. Return false to let it go. */
export type LeaveGuard = (proceed: () => void) => boolean;

/** What the frame hands every group page. */
export interface SettingsPageProps {
  /** Where the page should land, from the nav, a deep link, or a
   *  search hit. The frame scrolls to the anchor itself. A page reads
   *  the parts that are state, like the Automation kind, the Characters
   *  profile, or a Disclosure that holds the anchor. */
  target: SettingsTarget;
  /** Goes up on every navigation, even to the same target, so a page
   *  can react to a second press of the same link. */
  navSeq: number;
  /** The active profile's UiConfig, or null until it loads. */
  config: UiConfig | null;
  setConfig: SetUiConfig;
  /** Show an error line above the page, or clear it with null. */
  onError: (message: string | null) => void;
  /** Loadout mode (Path B) is on. */
  pathB: boolean;
  /** Go somewhere else in Settings, the way the nav and search do. */
  navigate: (target: SettingsTarget) => void;
  /** Register the guard the frame asks before it leaves this page, or
   *  clear it with null. A page with a save bar uses it to ask about
   *  unsaved changes. Clear it when the page unmounts. */
  setLeaveGuard: (guard: LeaveGuard | null) => void;
}
