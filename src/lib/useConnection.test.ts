import { describe, expect, it } from 'vitest';
import { KNOWN_WORLDS, parseTarget, worldName } from './useConnection';

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

describe('parseTarget', () => {
  it('keeps a host, a TCP port, and the TLS flag', () => {
    expect(parseTarget({ host: ' mud.example.org ', port: 4000, tls: true })).toEqual({
      host: 'mud.example.org',
      port: 4000,
      tls: true,
    });
    expect(parseTarget({ host: 'mud.example.org', port: '23' })).toEqual({
      host: 'mud.example.org',
      port: 23,
      tls: false,
    });
  });

  it('rejects a blank host or a port out of range', () => {
    expect(parseTarget({ host: '  ', port: 23 })).toBeNull();
    expect(parseTarget({ host: 'mud.example.org', port: 0 })).toBeNull();
    expect(parseTarget({ host: 'mud.example.org', port: 70000 })).toBeNull();
    expect(parseTarget({ host: 'mud.example.org', port: 23.5 })).toBeNull();
    expect(parseTarget(null)).toBeNull();
  });
});
