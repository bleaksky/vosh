import { describe, expect, it } from 'vitest';
import { KNOWN_WORLDS, knownWorld, worldLabel, worldName } from './knownWorlds';

describe('knownWorld', () => {
  it('finds the world a host plays', () => {
    expect(knownWorld('play.theforsakenlands.com')?.name).toBe('The Forsaken Lands');
    expect(knownWorld('mud.example.org')).toBeUndefined();
  });
});

describe('KNOWN_WORLDS', () => {
  it('knows where to connect to The Forsaken Lands', () => {
    expect(KNOWN_WORLDS).toContainEqual({
      domain: 'theforsakenlands.com',
      name: 'The Forsaken Lands',
      host: 'play.theforsakenlands.com',
      port: 1848,
    });
  });

  it('names every known world by its own host', () => {
    for (const world of KNOWN_WORLDS) expect(worldName(world.host)).toBe(world.name);
  });
});

describe('worldName', () => {
  it('names a known world by its host or any subdomain', () => {
    expect(worldName('play.theforsakenlands.com')).toBe('The Forsaken Lands');
    expect(worldName('theforsakenlands.com')).toBe('The Forsaken Lands');
    expect(worldName(' Play.TheForsakenLands.com. ')).toBe('The Forsaken Lands');
  });

  it('shows any other host as typed', () => {
    expect(worldName('mud.example.org')).toBe('mud.example.org');
    expect(worldName('nottheforsakenlands.com')).toBe('nottheforsakenlands.com');
  });
});

describe('worldLabel', () => {
  it('adds a port that is not the world own', () => {
    expect(worldLabel('play.theforsakenlands.com', 1848)).toBe('The Forsaken Lands');
    expect(worldLabel('play.theforsakenlands.com', 1825)).toBe('The Forsaken Lands 1825');
  });

  it('shows any other host as typed, with no port', () => {
    expect(worldLabel(' mud.example.org ', 4000)).toBe('mud.example.org');
  });
});
