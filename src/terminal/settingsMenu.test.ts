import { describe, expect, it } from 'vitest';
import golden from '../../fixtures/links/settings-anchors.json';
import APP_SHORTCUTS from '../lib/appShortcuts.json';
import { SETTINGS_MENU } from './settingsMenu';
import {
  formatSettingsTarget,
  resolveSettingsTarget,
  SETTINGS_GROUPS,
  type SettingsTarget,
} from '../lib/settingsNav';
import { SETTINGS_ROWS, settingsRowKey } from '../settings/settingsSearch';

// The Settings list in the terminal menu sends the same deep links the
// palette and search send, so it opens Settings where they do. The
// golden file pins where each of those strings lands and what the page
// draws there, so a row whose link the file names is covered by the
// Settings links test.

const rows = SETTINGS_MENU.flat();
const lists = SETTINGS_MENU[0];
const pages = SETTINGS_MENU[1];
const at = (link: string | null): SettingsTarget | null =>
  link === null ? null : resolveSettingsTarget(link);

/** Every link string the golden file names. */
const PINNED = new Set<string>([
  ...golden.search,
  ...Object.values(golden.links).flatMap((group) => Object.keys(group)),
]);

describe('the Settings list in the terminal menu', () => {
  it('lists the four Automation lists, the eleven pages, then Help', () => {
    expect(SETTINGS_MENU.map((group) => group.map((row) => row.label))).toEqual([
      ['Triggers', 'Aliases', 'Macros', 'Timers'],
      [
        'General',
        'Appearance',
        'Accessibility',
        'Layout',
        'Vitals',
        'Prompt',
        'Input',
        'Automation',
        'Scripts',
        'Logs',
        'Characters',
      ],
      ['Help'],
    ]);
  });

  it('names each page as the Settings sidebar does, in its order', () => {
    expect(pages.map((row) => row.label)).toEqual(SETTINGS_GROUPS.map((g) => g.label));
    // The bare link logs opens the search, so the Logs row opens the
    // tab on its first section.
    expect(pages.map((row) => at(row.link))).toEqual(
      SETTINGS_GROUPS.map((g) =>
        g.id === 'logs' ? { group: g.id, section: 'session-logs' } : { group: g.id },
      ),
    );
  });

  it('names each list as Settings does, and opens Automation on it', () => {
    for (const row of lists) {
      const target = at(row.link);
      expect(target, row.label).toEqual({ group: 'automation', section: row.id });
      // The search index holds the label the page shows for each place.
      const shown = SETTINGS_ROWS.find((r) => settingsRowKey(r) === row.link);
      expect(shown?.label, row.label).toBe(row.label);
    }
  });

  it('opens each list where the palette row for it does', () => {
    const palette: Record<string, string> = golden.palette;
    for (const row of lists) {
      const sent = palette[`settings-${row.link}`];
      expect(sent, row.label).toBeDefined();
      expect(formatSettingsTarget(resolveSettingsTarget(sent)), row.label).toBe(row.link);
    }
  });

  it('sends only links the golden file pins', () => {
    for (const row of rows) {
      if (row.link === null) continue;
      expect(PINNED, row.label).toContain(row.link);
      // The string reads back as itself, so Settings lands on it as sent.
      expect(formatSettingsTarget(resolveSettingsTarget(row.link)), row.label).toBe(row.link);
    }
  });

  it('opens Help with its shortcut beside it', () => {
    const help = rows.at(-1);
    expect(help).toEqual({ id: 'help', label: 'Help', link: null, keys: APP_SHORTCUTS.help });
    expect(rows.filter((row) => row.link === null)).toHaveLength(1);
  });
});
