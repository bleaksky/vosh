import APP_SHORTCUTS from '../lib/appShortcuts.json';
import { formatSettingsTarget, SETTINGS_GROUPS } from '../lib/settingsNav';

// The Settings list in the terminal right-click menu. It goes straight
// to the four Automation lists, then to each Settings page, then to
// Help, in three groups split by separators.

/** A row of the Settings list. */
export interface SettingsMenuRow {
  id: string;
  /** The name Settings shows for the list or the page. */
  label: string;
  /** The Settings deep link the row opens, or null for Help. */
  link: string | null;
  /** Shortcut spec for the trailing hint. */
  keys?: string;
}

// The lists in the order the Automation kind switcher shows them.
const AUTOMATION_LISTS: readonly { id: string; label: string }[] = [
  { id: 'triggers', label: 'Triggers' },
  { id: 'aliases', label: 'Aliases' },
  { id: 'macros', label: 'Macros' },
  { id: 'timers', label: 'Timers' },
];

/** The Automation lists, the eleven Settings pages in the order the
 *  Settings sidebar lists them, then Help. Each Settings row sends the
 *  deep link search sends for the same place, which is where the
 *  palette row for it lands too, so Settings opens there, comes forward,
 *  and scrolls the same way. The bare link `logs` opens the search, so
 *  the Logs row names the tab's first section. */
export const SETTINGS_MENU: readonly (readonly SettingsMenuRow[])[] = [
  AUTOMATION_LISTS.map(({ id, label }) => ({
    id,
    label,
    link: formatSettingsTarget({ group: 'automation', section: id }),
  })),
  SETTINGS_GROUPS.map(({ id, label }) => ({
    id,
    label,
    link: formatSettingsTarget(
      id === 'logs' ? { group: id, section: 'session-logs' } : { group: id },
    ),
  })),
  [{ id: 'help', label: 'Help', link: null, keys: APP_SHORTCUTS.help }],
];
