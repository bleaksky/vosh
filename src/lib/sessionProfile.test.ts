import { describe, expect, it } from 'vitest';
import type { ProfileEntry } from '../ipc/profiles';
import type { SessionRow } from '../ipc/session';
import { profileLines, showsProfileRow } from './sessionProfile';

// The Profile row of the New session form, board 4. The line under the
// row says why the pick shows, or names the other session that plays
// it, or notes another session already connected to the world's own
// port. Rust makes the pick itself, and the tests of
// resolve_before_login in src-tauri/src/profile/login_match.rs hold it.

const PLAY = 'play.theforsakenlands.com';

const profile = (name: string, auto_match: ProfileEntry['auto_match'] = null): ProfileEntry => ({
  name,
  auto_match,
});

/** Default's claim saved from Characters is pinned to 1848. Build is
 *  pinned to the build port, and Healer claims the host on any port. */
const DEFAULT = profile('default', { host: PLAY, port: 1848, characters: ['Tolliver'] });
const BUILD = profile('Build', { host: PLAY, port: 1825, characters: ['Orla'] });
const HEALER = profile('Healer', { host: PLAY, port: null, characters: ['Maren'] });

function row(id: number, fields: Partial<SessionRow> = {}): SessionRow {
  return {
    id,
    name: null,
    character: null,
    host: null,
    port: null,
    tls: false,
    profile: 'default',
    connected: false,
    since: null,
    selected: false,
    ...fields,
  };
}

/** Tolliver plays Default on the world port. Session 2 is the new one. */
const TOLLIVER = row(1, { character: 'Tolliver', host: PLAY, port: 1848, connected: true });
const NEW = 2;

describe('showsProfileRow', () => {
  it('shows with more than one profile', () => {
    expect(showsProfileRow([DEFAULT, BUILD], [row(NEW)], NEW, 'default')).toBe(true);
  });

  it('shows one profile only while another session plays it', () => {
    expect(showsProfileRow([DEFAULT], [TOLLIVER, row(NEW)], NEW, 'default')).toBe(true);
    expect(showsProfileRow([DEFAULT], [row(NEW)], NEW, 'default')).toBe(false);
  });
});

describe('profileLines', () => {
  const lines = (profiles: ProfileEntry[], rows: SessionRow[], pick: string, port: number) =>
    profileLines(profiles, rows, NEW, pick, PLAY, port);

  it('says the profile is pinned to this world and port', () => {
    const rows = [TOLLIVER, row(NEW, { profile: 'Build' })];
    expect(lines([DEFAULT, BUILD], rows, 'Build', 1825)).toEqual({
      hint: 'Build is pinned to The Forsaken Lands 1825.',
      warn: null,
    });
    const pinned = profile('Build', { host: 'mud.example.org', port: 4000 });
    expect(profileLines([pinned], [row(NEW)], NEW, 'Build', 'mud.example.org', 4000).hint).toBe(
      'Build is pinned to mud.example.org:4000.',
    );
  });

  it('names the other session that plays the profile', () => {
    const rows = [TOLLIVER, row(NEW)];
    expect(lines([DEFAULT], rows, 'default', 1825)).toEqual({
      hint: "Tolliver's session plays Default too. An edit in either reaches both.",
      warn: null,
    });
  });

  it('names every other session that plays it', () => {
    const orla = row(3, { character: 'Orla', host: PLAY, port: 1825, connected: true });
    expect(lines([DEFAULT], [TOLLIVER, orla, row(NEW)], 'default', 4000).hint).toBe(
      "Tolliver's and Orla's sessions play Default too. An edit in any of them reaches all.",
    );
    const login = row(3, { host: PLAY, port: 1825 });
    expect(lines([DEFAULT], [login, row(NEW)], 'default', 4000).hint).toBe(
      'Another session plays Default too. An edit in either reaches both.',
    );
  });

  it('notes another session connected to this world and port, and never refuses', () => {
    const rows = [TOLLIVER, row(NEW, { profile: 'Healer' })];
    expect(lines([DEFAULT, HEALER], rows, 'Healer', 1848)).toEqual({
      hint: null,
      warn: 'Tolliver is connected to this world. HELP MULTI lists “Having more than one character logged on at once.”',
    });
  });

  it('notes it beside the line about a shared profile', () => {
    const { hint, warn } = lines([DEFAULT], [TOLLIVER, row(NEW)], 'default', 1848);
    expect(hint).toBe("Tolliver's session plays Default too. An edit in either reaches both.");
    expect(warn).toMatch(/^Tolliver is connected to this world\./);
  });

  it('says nothing when no line applies', () => {
    const rows = [TOLLIVER, row(NEW, { profile: 'Healer' })];
    expect(lines([DEFAULT, HEALER], rows, 'Healer', 1825)).toEqual({ hint: null, warn: null });
  });

  it('leaves the note out on the build port, where the game keeps other player files', () => {
    const builder = row(1, { character: 'Orla', host: PLAY, port: 1825, connected: true });
    const rows = [builder, row(NEW, { profile: 'Healer' })];
    expect(lines([DEFAULT, HEALER], rows, 'Healer', 1825)).toEqual({ hint: null, warn: null });
  });

  it('leaves the note out for another port, a session not connected, or a world Vosh does not know', () => {
    const away = { ...TOLLIVER, connected: false };
    expect(lines([DEFAULT, HEALER], [away, row(NEW)], 'Healer', 1848).warn).toBeNull();
    const elsewhere = row(1, {
      character: 'Maren',
      host: 'mud.example.org',
      port: 4000,
      connected: true,
    });
    expect(
      profileLines([DEFAULT, HEALER], [elsewhere, row(NEW)], NEW, 'Healer', 'mud.example.org', 4000)
        .warn,
    ).toBeNull();
  });
});
