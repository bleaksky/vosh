import { describe, expect, it } from 'vitest';
import {
  copyName,
  findProfileName,
  formatCharacterNames,
  hasWorld,
  keepsProfileName,
  loginCharacter,
  loginLabel,
  loginSentence,
  movedSentence,
  newProfileClaim,
  newProfileName,
  NO_WORLD,
  parseCharacterNames,
  parsePort,
  playedProfiles,
  profileDisplayName,
  profileWorld,
  sessionsSentence,
  takenProfileName,
  takenSentence,
  worldKey,
  worldOptions,
  worldSources,
} from './characterProfiles';
import { possessive } from './text';
import type { LoginClaim, SessionIdentity } from '../ipc/characters';
import type { ProfileEntry } from '../ipc/profiles';
import type { SessionRow } from '../ipc/session';

const TFL = 'play.theforsakenlands.com';

// James's index: default claims Ilsabet, Healer claims Corvanne, and
// Test-Prompt claims Ilsabet too.
const JAMES: ProfileEntry[] = [
  {
    name: 'default',
    description: 'Immortal',
    auto_match: { host: TFL, port: 1848, characters: ['Ilsabet'] },
  },
  { name: 'Healer', auto_match: { host: TFL, port: 1848, characters: ['Corvanne'] } },
  { name: 'Test-Prompt', auto_match: { host: TFL, port: 1848, characters: ['Ilsabet'] } },
];
const NAMES = JAMES.map((p) => p.name);

const identity = (character: string | null): SessionIdentity => ({
  host: TFL,
  port: 1848,
  character,
  profile: 'default',
  claimed_by: character === 'Ilsabet' ? 'default' : null,
});

describe('profile names', () => {
  it('shows Default for the reserved profile', () => {
    expect(profileDisplayName('default')).toBe('Default');
    expect(profileDisplayName('Healer')).toBe('Healer');
    expect(profileDisplayName('Default')).toBe('Default');
  });

  it('keeps every other name as it is', () => {
    expect(profileDisplayName('Ilsabet')).toBe('Ilsabet');
    expect(profileDisplayName('aabahran-ilsabet')).toBe('aabahran-ilsabet');
  });

  it('reads the reserved profile as Default in a possessive', () => {
    expect(possessive(profileDisplayName('default'))).toBe("Default's");
  });

  it('finds the profile a deep link names in any case or by display name', () => {
    expect(findProfileName(NAMES, 'Healer')).toBe('Healer');
    expect(findProfileName(NAMES, 'healer')).toBe('Healer');
    expect(findProfileName(NAMES, 'Default')).toBe('default');
    expect(findProfileName(NAMES, ' test-prompt ')).toBe('Test-Prompt');
    expect(findProfileName(NAMES, 'Nobody')).toBeNull();
    expect(findProfileName(NAMES, '')).toBeNull();
  });

  it('suggests the logged in character for a new profile when the name is free', () => {
    expect(newProfileName(identity('Ondrevar'), NAMES)).toBe('Ondrevar');
    expect(newProfileName(identity('healer'), NAMES)).toBe('');
    expect(newProfileName(identity(null), NAMES)).toBe('');
    expect(newProfileName(null, NAMES)).toBe('');
  });

  it('finds a free name for a copy', () => {
    expect(copyName('Healer', NAMES)).toBe('Healer copy');
    expect(copyName('default', NAMES)).toBe('Default copy');
    expect(copyName('Healer', [...NAMES, 'Healer copy', 'healer copy 2'])).toBe('Healer copy 3');
  });

  it('treats a name another profile has in any case as taken', () => {
    // Characters shows default as Default, and the disk keeps
    // Default.toml and default.toml as one file.
    expect(takenProfileName(NAMES, 'Default')).toBe('default');
    expect(takenProfileName(NAMES, ' HEALER ')).toBe('Healer');
    expect(takenProfileName(NAMES, 'test-prompt', 'Healer')).toBe('Test-Prompt');
    expect(takenProfileName(NAMES, 'Default', 'Healer')).toBe('default');
    expect(takenProfileName(NAMES, 'Ondrevar')).toBeNull();
    expect(takenProfileName(NAMES, '  ')).toBeNull();
    // A profile may change the case of its own name.
    expect(takenProfileName(NAMES, 'healer', 'Healer')).toBeNull();
    expect(takenSentence('default')).toBe('You already have a profile named Default.');
    expect(takenSentence('Healer')).toBe('You already have a profile named Healer.');
  });

  it('knows a rename that keeps the name as it reads', () => {
    expect(keepsProfileName('default', 'Default')).toBe(true);
    expect(keepsProfileName('default', ' default ')).toBe(true);
    expect(keepsProfileName('Healer', 'Healer')).toBe(true);
    expect(keepsProfileName('Healer', 'healer')).toBe(false);
    expect(keepsProfileName('default', 'Main')).toBe(false);
  });
});

describe('the login toggle', () => {
  it('names the first character, else the one logged in, else nobody', () => {
    expect(loginCharacter(JAMES[0].auto_match, identity('Ondrevar'))).toBe('Ilsabet');
    expect(loginCharacter({ host: TFL, characters: [' ', ''] }, identity('Ondrevar'))).toBe(
      'Ondrevar',
    );
    expect(loginCharacter(null, identity(null))).toBeNull();
    expect(loginCharacter(undefined, null)).toBeNull();
  });

  it('reads as a sentence with or without the name', () => {
    expect(loginLabel('Ilsabet')).toBe('Use this profile when you log in as Ilsabet');
    expect(loginLabel(null)).toBe('Use this profile when you log in');
  });

  it('knows whether a profile has a world', () => {
    expect(hasWorld(JAMES[0].auto_match)).toBe(true);
    expect(hasWorld({ host: '  ', characters: [] })).toBe(false);
    expect(hasWorld(null)).toBe(false);
  });

  it('reports every profile a claim took the character from', () => {
    expect(movedSentence('Ilsabet', ['Test-Prompt'], 'Ilsabet')).toBe(
      'Vosh moved Ilsabet from Test-Prompt to Ilsabet.',
    );
    expect(movedSentence('Ilsabet', ['default', 'Test-Prompt'], 'Ilsabet')).toBe(
      'Vosh moved Ilsabet from Default and Test-Prompt to Ilsabet.',
    );
    expect(movedSentence('Ilsabet', ['a', 'b', 'c'], 'default')).toBe(
      'Vosh moved Ilsabet from a, b, and c to Default.',
    );
    expect(movedSentence('Ilsabet', [], 'default')).toBeNull();
  });
});

describe('the World select', () => {
  it('treats every spelling of a known world on its port as one choice', () => {
    expect(worldKey(TFL, 1848)).toBe('world:theforsakenlands.com');
    expect(worldKey('TheForsakenLands.com.', null)).toBe('world:theforsakenlands.com');
    expect(worldKey(TFL, 4000)).toBe('host:play.theforsakenlands.com:4000');
    expect(worldKey('mud.example.org', 4000)).toBe('host:mud.example.org:4000');
    expect(worldKey('  ', 4000)).toBe(NO_WORLD);
    expect(worldKey(null, null)).toBe(NO_WORLD);
  });

  it('lists known worlds, then other hosts once each, then No world', () => {
    const options = worldOptions(
      worldSources(
        [
          ...JAMES,
          { name: 'Other', auto_match: { host: 'mud.example.org', port: 4000, characters: [] } },
          { name: 'Blank', auto_match: null },
        ],
        { host: 'MUD.example.org', port: 4000 },
        { ...identity('Ilsabet'), host: 'localhost', port: 4000 },
      ),
    );
    expect(options).toEqual([
      { value: 'world:theforsakenlands.com', label: 'The Forsaken Lands', host: TFL, port: 1848 },
      {
        value: 'host:mud.example.org:4000',
        label: 'mud.example.org:4000',
        host: 'mud.example.org',
        port: 4000,
      },
      { value: 'host:localhost:4000', label: 'localhost:4000', host: 'localhost', port: 4000 },
      { value: NO_WORLD, label: 'No world', host: null, port: null },
    ]);
  });

  it('labels a known world on another port as its row reads', () => {
    const options = worldOptions([
      { host: TFL, port: 1825 },
      { host: 'mud.example.org', port: 1825 },
    ]);
    expect(options.map((o) => o.label)).toEqual([
      'The Forsaken Lands',
      'The Forsaken Lands 1825',
      'mud.example.org:1825',
      'No world',
    ]);
  });

  it('shows a profile row its world by name', () => {
    expect(profileWorld(JAMES[0])).toEqual({ world: 'The Forsaken Lands', port: null });
    expect(
      profileWorld({ name: 'x', auto_match: { host: 'mud.example.org', characters: [] } }),
    ).toEqual({ world: 'mud.example.org', port: null });
    expect(profileWorld({ name: 'y', auto_match: null })).toBeNull();
  });

  it("adds the port of a known world when it is not the world's own", () => {
    const on = (port: number | null) =>
      profileWorld({ name: 'Build', auto_match: { host: TFL, port, characters: [] } });
    expect(on(1825)).toEqual({ world: 'The Forsaken Lands', port: 1825 });
    expect(on(1848)).toEqual({ world: 'The Forsaken Lands', port: null });
    expect(on(null)).toEqual({ world: 'The Forsaken Lands', port: null });
    expect(
      profileWorld({ name: 'x', auto_match: { host: 'mud.example.org', port: 4000 } }),
    ).toEqual({ world: 'mud.example.org', port: null });
  });
});

describe('a new profile', () => {
  it('claims the world and character you are logged in as, toggle off until claimed', () => {
    expect(newProfileClaim(identity('Ondrevar'), JAMES[0])).toEqual({
      host: TFL,
      port: 1848,
      characters: ['Ondrevar'],
      enabled: false,
    });
  });

  it('takes the active profile world when you are not connected', () => {
    expect(newProfileClaim(null, JAMES[1])).toEqual({
      host: TFL,
      port: 1848,
      characters: [],
      enabled: false,
    });
    expect(newProfileClaim(null, { name: 'Blank', auto_match: null })).toBeNull();
    expect(newProfileClaim(null, undefined)).toBeNull();
  });
});

describe('typed values', () => {
  it('reads character names as a list', () => {
    expect(parseCharacterNames(' Ilsabet, Thessamy ,, ilsabet, Ondrevar ')).toEqual([
      'Ilsabet',
      'Thessamy',
      'Ondrevar',
    ]);
    expect(parseCharacterNames('')).toEqual([]);
    expect(formatCharacterNames(['Ilsabet', 'Thessamy'])).toBe('Ilsabet, Thessamy');
    expect(formatCharacterNames(undefined)).toBe('');
  });

  it('reads a port, blank as none', () => {
    expect(parsePort(' 1848 ')).toBe(1848);
    expect(parsePort('')).toBeNull();
    expect(parsePort('0')).toBeUndefined();
    expect(parsePort('70000')).toBeUndefined();
    expect(parsePort('18a')).toBeUndefined();
  });
});

// Board 7 and board 9 of the Sessions review: Default, Build on the build
// port and Healer, with Tolliver on the play port.
const BOARD: ProfileEntry[] = [
  { name: 'default', auto_match: { host: TFL, port: 1848, characters: ['Tolliver'] } },
  { name: 'Build', auto_match: { host: TFL, port: 1825, characters: ['Orla'] } },
  { name: 'Healer', auto_match: { host: TFL, port: null, characters: ['Maren'] } },
];

const session = (id: number, patch: Partial<SessionRow>): SessionRow => ({
  id,
  name: null,
  character: 'Tolliver',
  host: TFL,
  port: 1848,
  tls: false,
  profile: 'default',
  connected: true,
  since: null,
  selected: id === 1,
  ...patch,
});
const TOLLIVER = session(1, {});
const ORLA = session(2, { character: 'Orla', port: 1825, profile: 'Build' });
const BUILDER = session(2, { name: 'Builder', port: 1825 });

describe('the profiles the sessions play', () => {
  it('marks each profile a session plays and dims Switch for the selected one', () => {
    const played = playedProfiles([TOLLIVER, ORLA], 2, 'Build');
    expect([...played.all]).toEqual(['default', 'Build']);
    expect(played.selected).toBe('Build');
    expect(playedProfiles([TOLLIVER, ORLA], 1, 'default').selected).toBe('default');
  });

  it('marks a shared profile once', () => {
    const played = playedProfiles([TOLLIVER, BUILDER], 2, 'default');
    expect([...played.all]).toEqual(['default']);
    expect(played.selected).toBe('default');
  });

  it('marks the profile in use before the first session list', () => {
    const played = playedProfiles([], 1, 'Healer');
    expect([...played.all]).toEqual(['Healer']);
    expect(played.selected).toBe('Healer');
  });
});

describe('the line under the Characters list', () => {
  it('says nothing with one session', () => {
    expect(sessionsSentence(BOARD, [TOLLIVER])).toBeNull();
    expect(sessionsSentence(BOARD, [])).toBeNull();
  });

  it('names the session on each profile, as board 7 draws it', () => {
    expect(sessionsSentence(BOARD, [TOLLIVER, ORLA])).toBe(
      "Default plays in Tolliver's session, Build in Orla's.",
    );
  });

  it('names every session on a shared profile, as board 9 draws it', () => {
    expect(sessionsSentence(BOARD, [TOLLIVER, BUILDER])).toBe(
      'Default plays in two sessions, Tolliver and Builder.',
    );
  });

  it('follows the list order of the profiles and the sidebar order of the sessions', () => {
    const maren = session(3, { character: 'Maren', port: 1848, profile: 'Healer' });
    expect(sessionsSentence(BOARD, [maren, ORLA, TOLLIVER])).toBe(
      "Default plays in Tolliver's session, Build in Orla's, Healer in Maren's.",
    );
  });

  it('gives each profile a sentence once one is shared', () => {
    const maren = session(3, { character: 'Maren', port: 1848 });
    expect(sessionsSentence(BOARD, [TOLLIVER, ORLA, maren])).toBe(
      "Default plays in two sessions, Tolliver and Maren. Build plays in Orla's session.",
    );
    const three = [TOLLIVER, maren, session(4, { character: 'Orla' })];
    expect(sessionsSentence(BOARD, three)).toBe(
      'Default plays in three sessions, Tolliver, Maren, and Orla.',
    );
  });

  it('adds the port where two sessions go by one name, as their rows do', () => {
    const build = session(2, { port: 1825, profile: 'Build' });
    expect(sessionsSentence(BOARD, [TOLLIVER, build])).toBe(
      "Default plays in Tolliver's session, Build in Tolliver's on 1825.",
    );
    const unnamed = session(2, { port: 1825 });
    expect(sessionsSentence(BOARD, [TOLLIVER, unnamed])).toBe(
      'Default plays in two sessions, Tolliver and Tolliver on 1825.',
    );
  });

  it('names a session at the login by where it plays', () => {
    const login = session(2, { character: null, port: 1825, profile: 'Build' });
    expect(sessionsSentence(BOARD, [TOLLIVER, login])).toBe(
      "Default plays in Tolliver's session, Build in a session on The Forsaken Lands 1825.",
    );
    const fresh = session(2, { character: null, host: null, port: null });
    expect(sessionsSentence(BOARD, [TOLLIVER, fresh])).toBe(
      'Default plays in two sessions, Tolliver and a new one.',
    );
    const shared = session(2, { character: null, port: 1825 });
    expect(sessionsSentence(BOARD, [TOLLIVER, shared])).toBe(
      'Default plays in two sessions, Tolliver and one on The Forsaken Lands 1825.',
    );
  });
});

describe('the line after a login pin', () => {
  // Board 7: Tolliver claimed for Build on the build port, where Default
  // held him on the host alone.
  const build = { name: 'Build', auto_match: { host: TFL, port: 1825, characters: ['Tolliver'] } };
  const claim = (pinned: LoginClaim['pinned'], released: string[] = []): LoginClaim => ({
    entry: build,
    released_from: released,
    pinned,
  });

  it('says where each profile plays the character and where the claim moved', () => {
    const pin = { profile: 'default', port: 1848, characters: ['Tolliver'] };
    expect(loginSentence('Tolliver', claim([pin]), 'Build')).toBe(
      "Tolliver plays Default on 1848 and Build on 1825. Default's claim now sits on 1848.",
    );
  });

  it('names every other character the pinned claim carried', () => {
    const one = { profile: 'default', port: 1848, characters: ['Tolliver', 'Maren'] };
    expect(loginSentence('Tolliver', claim([one]), 'Build')).toBe(
      "Tolliver plays Default on 1848 and Build on 1825. Default's claim now sits on 1848, and Maren moved with it.",
    );
    const two = { profile: 'default', port: 1848, characters: ['tolliver', 'Maren', 'Orla'] };
    expect(loginSentence('Tolliver', claim([two]), 'Build')).toBe(
      "Tolliver plays Default on 1848 and Build on 1825. Default's claim now sits on 1848, and Maren and Orla moved with it.",
    );
  });

  it('names every profile a pin moved, then any it took the character from', () => {
    const pins = [
      { profile: 'default', port: 1848, characters: ['Tolliver'] },
      { profile: 'Healer', port: 1848, characters: ['Tolliver', 'Orla'] },
    ];
    expect(loginSentence('Tolliver', claim(pins, ['Spare']), 'Build')).toBe(
      'Tolliver plays Default and Healer on 1848 and Build on 1825. ' +
        "Default's claim now sits on 1848. Healer's claim now sits on 1848, and Orla moved with it. " +
        'Vosh moved Tolliver from Spare to Build.',
    );
  });

  it('says only what a toggle with no pin did', () => {
    expect(loginSentence('Tolliver', claim([], ['default']), 'Build')).toBe(
      'Vosh moved Tolliver from Default to Build.',
    );
    expect(loginSentence('Tolliver', claim([]), 'Build')).toBeNull();
  });
});
