// The five alert presets, the Alerts category of the Presets page
// (Alerts Q5, board 2). They hold no triggers. Rust listens for each one
// in src-tauri/src/alert/presets.rs, and the profile's `[alerts]` table
// says what each does. They sit apart from the library in presets.ts,
// which installs triggers and macros, so the wizard and Get started
// never offer them.

import type { AlertParts } from '../ipc/automation';

export interface AlertPreset {
  id: string;
  name: string;
  description: string;
  /** What the game or the link tells Vosh that rings it, in plain
   *  words. */
  listensTo: string;
  /** Rust passes the banner the words of a tell or of the line, so
   *  Banner shows has a choice to make. The other three ring a title
   *  alone. */
  words: boolean;
}

/** The presets in the order of PRESETS in src-tauri/src/alert/presets.rs,
 *  which a test there holds this list to. */
export const ALERT_PRESETS: readonly AlertPreset[] = [
  {
    id: 'alert_tells',
    name: 'Tells you get',
    description: 'Gets your attention when someone sends you a tell.',
    listensTo: "The game's word that a tell reached you",
    words: true,
  },
  {
    id: 'alert_name',
    name: 'Your name',
    description: 'Gets your attention when a line from the game names you.',
    listensTo: 'Each line the game sends',
    words: true,
  },
  {
    id: 'alert_attacked',
    name: 'Being attacked',
    description: 'Gets your attention when someone starts a fight with you.',
    listensTo: "The game's word that a fight began on you",
    words: false,
  },
  {
    id: 'alert_low_health',
    name: 'Low health',
    description: 'Gets your attention when your health falls under 20 percent.',
    listensTo: 'Your health as the game reports it',
    words: false,
  },
  {
    id: 'alert_connection',
    name: 'Connection',
    description:
      'Gets your attention when your link to the game drops, when the game waits for you to log in, and when Vosh stops trying.',
    listensTo: 'Your link to the game',
    words: false,
  },
];

/** What a preset does while the `[alerts]` table names no parts for it,
 *  as Rust `parts()` reads it: a banner of the title alone, only while
 *  you are not looking at its session. */
export const PRESET_ALERT_DEFAULT: AlertParts = { banner: true, background: true, words: false };

const IDS = new Set(ALERT_PRESETS.map((p) => p.id));

export function isAlertPresetId(id: string): boolean {
  return IDS.has(id);
}
