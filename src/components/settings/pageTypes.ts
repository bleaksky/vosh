import type { UiConfig } from '../../lib/session';
import type { SettingsTarget } from '../../lib/settingsNav';

/** Replace the window's UiConfig copy. Every save sends the whole
 *  snapshot, so a page edits through this and never keeps its own
 *  copy of the config. */
export type SetUiConfig = (updater: (prev: UiConfig | null) => UiConfig | null) => void;

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
}
