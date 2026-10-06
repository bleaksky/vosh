import { describe, expect, it } from 'vitest';
import type { ImportPreview, ImportResult } from '../../ipc/characters';
import type { ProfileEntry } from '../../ipc/profiles';
import { claimNote, importedSentence, importSummary, type SummaryRow } from './profileImport';

// What the import sheet says about a profile export (board 5 of the
// Scripts design), from the preview Rust gives for the full golden
// export (src-tauri/src/import/vosh.rs) and the results it returns.

const WORLD = 'play.theforsakenlands.com';

/** fixtures/config/export.full.toml picked as Healer profile.toml, with
 *  Healer keeping Orla. */
const FULL: ImportPreview = {
  name: 'Healer',
  triggers: 3,
  aliases: 2,
  macros: 2,
  timers: 1,
  tick: true,
  variables: 2,
  panes: ['map', 'chat', 'group'],
  runs_lua: [
    { kind: 'trigger', name: 'tells' },
    { kind: 'alias', name: 'heal' },
  ],
  plugins: ['vitals_alert'],
  world: { host: WORLD, port: 1848, name: 'The Forsaken Lands' },
  characters: [{ name: 'Orla', claimed_by: 'Healer' }],
};

/** Each row as it reads, with the names in the MUD font in brackets. */
function read(rows: SummaryRow[]): string[] {
  return rows.map(
    (row) =>
      `${row.label}: ${row.value.map((p) => (typeof p === 'string' ? p : `[${p.mono}]`)).join('')}${row.wide ? ' (wide)' : ''}`,
  );
}

const result = (patch: Partial<ImportResult> = {}): ImportResult => ({
  name: 'Healer 2',
  moved_from: [],
  kept_with: [],
  catalog_group: null,
  clashes: [],
  ...patch,
});

describe('In this file', () => {
  it('says what profile.full.toml holds, as board 5 draws it', () => {
    expect(read(importSummary(FULL))).toEqual([
      'Triggers: 3',
      'Aliases: 2',
      'Macros: 2',
      'Timers: 1 and the tick',
      'Variables: 2',
      'Panes: Map, Chat, Group',
      'Runs Lua: Trigger [tells], alias [heal] (wide)',
      'Plugins: [vitals_alert], off until you turn it on (wide)',
    ]);
  });

  it('leaves out Runs Lua and Plugins for a file with neither, and the tick at its default', () => {
    const plain = { ...FULL, runs_lua: [], plugins: [], tick: false, panes: ['map', 'affects'] };
    expect(read(importSummary(plain))).toEqual([
      'Triggers: 3',
      'Aliases: 2',
      'Macros: 2',
      'Timers: 1',
      'Variables: 2',
      'Panes: Map, Affects',
    ]);
  });

  it('names every Lua item and plugin, and a tick with no timer', () => {
    const more: ImportPreview = {
      ...FULL,
      timers: 0,
      runs_lua: [
        { kind: 'trigger', name: 'tells' },
        { kind: 'trigger', name: 'fled' },
        { kind: 'alias', name: 'heal' },
      ],
      plugins: ['vitals_alert', 'weather_pane'],
      panes: [],
    };
    expect(read(importSummary(more)).slice(3)).toEqual([
      'Timers: The tick',
      'Variables: 2',
      'Panes: None',
      'Runs Lua: Trigger [tells], trigger [fled], alias [heal] (wide)',
      'Plugins: [vitals_alert], [weather_pane], off until you turn them on (wide)',
    ]);
  });
});

describe('the note under a claimed character', () => {
  const healer = (patch: Partial<NonNullable<ProfileEntry['auto_match']>>): ProfileEntry[] => [
    { name: 'default', auto_match: { host: WORLD, port: 1848, characters: [] } },
    { name: 'Healer', auto_match: { host: WORLD, port: 1848, characters: ['Orla'], ...patch } },
  ];

  it('says the login turns off when the character is the last one', () => {
    expect(claimNote('Orla', 'Healer', healer({}))).toBe(
      "Healer uses Orla now. Turn this on to move Orla here, and Healer's login turns off.",
    );
  });

  it('leaves the login alone for a profile with another character, or one already off', () => {
    const plain = 'Healer uses Orla now. Turn this on to move Orla here.';
    expect(claimNote('Orla', 'Healer', healer({ characters: ['Orla', 'Maren'] }))).toBe(plain);
    expect(claimNote('Orla', 'Healer', healer({ enabled: false }))).toBe(plain);
  });

  it('names the reserved profile as Default', () => {
    const profiles: ProfileEntry[] = [
      { name: 'default', auto_match: { host: WORLD, characters: ['Orla'] } },
    ];
    expect(claimNote('Orla', 'default', profiles)).toBe(
      "Default uses Orla now. Turn this on to move Orla here, and Default's login turns off.",
    );
  });
});

describe('the line under the list once the import is done', () => {
  const FILE = 'Healer profile.toml';

  it('reads each result sentence of board 5 word for word', () => {
    expect(
      importedSentence(
        FILE,
        'new',
        result({ kept_with: [{ character: 'Orla', profile: 'Healer' }] }),
        [],
      ),
    ).toBe(
      'Vosh added Healer 2 from Healer profile.toml. Orla stays with Healer, so Healer 2 starts with its login off.',
    );
    expect(
      importedSentence(
        FILE,
        'new',
        result({ moved_from: [{ character: 'Orla', profile: 'Healer', login_off: true }] }),
        ['Orla'],
      ),
    ).toBe(
      "Vosh added Healer 2 from Healer profile.toml. Orla now uses Healer 2, and Healer's login is off.",
    );
    expect(importedSentence(FILE, 'replace', result({ name: 'Healer' }), [])).toBe(
      'Vosh replaced Healer with Healer profile.toml. Healer keeps its world and characters.',
    );
    expect(importedSentence(FILE, 'new', result({ catalog_group: 'Healer profile' }), [])).toBe(
      'Vosh added Healer 2 from Healer profile.toml. Its triggers, aliases and macros joined the catalog in the group Healer profile.',
    );
  });

  it('keeps the login out of a kept character when the new profile took another', () => {
    expect(
      importedSentence(
        FILE,
        'new',
        result({ kept_with: [{ character: 'Orla', profile: 'Healer' }] }),
        ['Maren'],
      ),
    ).toBe('Vosh added Healer 2 from Healer profile.toml. Orla stays with Healer.');
  });

  it('says nothing of the login of a profile a move left with a character', () => {
    expect(
      importedSentence(
        FILE,
        'new',
        result({
          moved_from: [
            { character: 'Orla', profile: 'Healer', login_off: false },
            { character: 'Maren', profile: 'Healer', login_off: false },
          ],
        }),
        ['Orla', 'Maren'],
      ),
    ).toBe('Vosh added Healer 2 from Healer profile.toml. Orla and Maren now use Healer 2.');
  });

  it('names the catalog group ahead of the characters, and Default by its name', () => {
    expect(
      importedSentence(
        FILE,
        'new',
        result({
          catalog_group: 'Healer profile',
          kept_with: [{ character: 'Orla', profile: 'default' }],
        }),
        [],
      ),
    ).toBe(
      'Vosh added Healer 2 from Healer profile.toml. Its triggers, aliases and macros joined the catalog in the group Healer profile. Orla stays with Default, so Healer 2 starts with its login off.',
    );
  });
});
