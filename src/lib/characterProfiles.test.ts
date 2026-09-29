import { describe, expect, it } from 'vitest';
import {
  copyName,
  findProfileName,
  formatCharacterNames,
  hasWorld,
  keepsProfileName,
  loginCharacter,
  loginLabel,
  movedSentence,
  newProfileClaim,
  newProfileName,
  NO_WORLD,
  parseCharacterNames,
  parsePort,
  profileDisplayName,
  profileWorldName,
  takenProfileName,
  takenSentence,
  worldKey,
  worldOptions,
  worldSources,
} from './characterProfiles';
import type { ProfileEntry, SessionIdentity } from './session';

const TFL = 'play.theforsakenlands.com';

// James's index: default claims Erelei, Healer claims Caelaor, and
// Test-Prompt claims Erelei too.
const JAMES: ProfileEntry[] = [
  {
    name: 'default',
    description: 'Immortal',
    auto_match: { host: TFL, port: 1848, characters: ['Erelei'] },
  },
  { name: 'Healer', auto_match: { host: TFL, port: 1848, characters: ['Caelaor'] } },
  { name: 'Test-Prompt', auto_match: { host: TFL, port: 1848, characters: ['Erelei'] } },
];
const NAMES = JAMES.map((p) => p.name);

const identity = (character: string | null): SessionIdentity => ({
  host: TFL,
  port: 1848,
  character,
  profile: 'default',
  claimed_by: character === 'Erelei' ? 'default' : null,
});

describe('profile names', () => {
  it('shows Default for the reserved profile', () => {
    expect(profileDisplayName('default')).toBe('Default');
    expect(profileDisplayName('Healer')).toBe('Healer');
    expect(profileDisplayName('Default')).toBe('Default');
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
    expect(newProfileName(identity('Vanek'), NAMES)).toBe('Vanek');
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
    expect(takenProfileName(NAMES, 'Vanek')).toBeNull();
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
    expect(loginCharacter(JAMES[0].auto_match, identity('Vanek'))).toBe('Erelei');
    expect(loginCharacter({ host: TFL, characters: [' ', ''] }, identity('Vanek'))).toBe('Vanek');
    expect(loginCharacter(null, identity(null))).toBeNull();
    expect(loginCharacter(undefined, null)).toBeNull();
  });

  it('reads as a sentence with or without the name', () => {
    expect(loginLabel('Erelei')).toBe('Use this profile when you log in as Erelei');
    expect(loginLabel(null)).toBe('Use this profile when you log in');
  });

  it('knows whether a profile has a world', () => {
    expect(hasWorld(JAMES[0].auto_match)).toBe(true);
    expect(hasWorld({ host: '  ', characters: [] })).toBe(false);
    expect(hasWorld(null)).toBe(false);
  });

  it('reports every profile a claim took the character from', () => {
    expect(movedSentence('Erelei', ['Test-Prompt'], 'Erelei')).toBe(
      'Vosh moved Erelei from Test-Prompt to Erelei.',
    );
    expect(movedSentence('Erelei', ['default', 'Test-Prompt'], 'Erelei')).toBe(
      'Vosh moved Erelei from Default and Test-Prompt to Erelei.',
    );
    expect(movedSentence('Erelei', ['a', 'b', 'c'], 'default')).toBe(
      'Vosh moved Erelei from a, b, and c to Default.',
    );
    expect(movedSentence('Erelei', [], 'default')).toBeNull();
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
        { ...identity('Erelei'), host: 'localhost', port: 4000 },
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

  it('shows a profile row its world by name', () => {
    expect(profileWorldName(JAMES[0])).toBe('The Forsaken Lands');
    expect(
      profileWorldName({ name: 'x', auto_match: { host: 'mud.example.org', characters: [] } }),
    ).toBe('mud.example.org');
    expect(profileWorldName({ name: 'y', auto_match: null })).toBeNull();
  });
});

describe('a new profile', () => {
  it('claims the world and character you are logged in as, toggle off until claimed', () => {
    expect(newProfileClaim(identity('Vanek'), JAMES[0])).toEqual({
      host: TFL,
      port: 1848,
      characters: ['Vanek'],
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
    expect(parseCharacterNames(' Erelei, Akletus ,, erelei, Vanek ')).toEqual([
      'Erelei',
      'Akletus',
      'Vanek',
    ]);
    expect(parseCharacterNames('')).toEqual([]);
    expect(formatCharacterNames(['Erelei', 'Akletus'])).toBe('Erelei, Akletus');
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
