import { describe, expect, it } from 'vitest';
import latch from '../../fixtures/alerts/low-latch.json';
import looks from '../../fixtures/room-colors/looks.json';
import type { SessionRow } from '../ipc/session';
import { parseCombat } from '../stores/gmcp/combatStore';
import { parseRoomInfo, type RoomInfoBase } from '../stores/gmcp/roomStore';
import { nextVitals, type VitalValues, type Vitals } from '../stores/gmcp/vitalsStore';
import type { SessionRowState } from '../stores/session/sessionRowStore';
import { aabahranPacket } from '../test/aabahranGmcp';
import { howLong, secondLine } from './sessionLine';

// Holds the second line of a session's row to board 02 of the Sessions
// Sidebar review, one string for each state it draws. The rooms and the
// target are the game's own, from the GMCP fixtures and the room looks.

const PLAY = 'play.theforsakenlands.com';
const NOW = 10_000_000;

const row = (fields: Partial<SessionRow> = {}): SessionRow => ({
  id: 2,
  name: null,
  character: 'Orla',
  host: PLAY,
  port: 1825,
  tls: false,
  profile: 'Build',
  connected: true,
  since: null,
  selected: false,
  ...fields,
});

const QUIET: SessionRowState = {
  link: null,
  redialing: false,
  reached: false,
  playing: false,
  lines: false,
  waiting: [],
  downAt: null,
  refused: false,
  try: null,
  tries: null,
};
const state = (fields: Partial<SessionRowState> = {}): SessionRowState => ({
  ...QUIET,
  ...fields,
});
const PLAYING = state({ link: 'live', playing: true });

/** The room of the Bank, from Room.Info. */
const BANK = parseRoomInfo(aabahranPacket('room-info.gmcp').data);

/** The room whose name the look of a murder of crows prints, its colors
 *  taken off. */
function woods(): RoomInfoBase | null {
  const lines = looks.cases.flatMap((c) => c.events.map((e) => ('line' in e ? e.line : '')));
  const plain = lines.map((line) =>
    line
      .split('\u001b')
      .map((part) => part.replace(/^\[[0-9;]*m/, ''))
      .join(''),
  );
  return parseRoomInfo({ name: plain.find((line) => line === 'Thickening Woods') });
}

const FULL: VitalValues = { hp: 0, maxhp: 0, mana: 760, maxmana: 820, move: 250, maxmove: 250 };
const health = (hp: number, maxhp: number, prev: Vitals | null = null) =>
  nextVitals(prev, { ...FULL, hp, maxhp });

describe('the second line of a session row', () => {
  it('reads the room you stand in, with your health', () => {
    expect(secondLine(row(), PLAYING, woods(), null, health(928, 1020), NOW)).toEqual({
      who: null,
      text: 'Thickening Woods',
      health: 91,
      low: false,
    });
  });

  it('reads who you fight while it lasts, in the danger tone on the low latch', () => {
    // The fall in the latch fixture's run, then a blow further down.
    const vitals = run(latch.runs[0].vitals.slice(0, 2));
    expect(vitals?.low.hp).toBe(true);
    const fight = parseCombat(aabahranPacket('char-combat.gmcp').data);
    expect(secondLine(row(), PLAYING, BANK, fight, health(162, 900, vitals), NOW)).toEqual({
      who: null,
      text: 'Fighting a Blackwatch guard',
      health: 18,
      low: true,
    });
    // The fight ends.
    const ended = parseCombat(aabahranPacket('char-combat-end.gmcp').data);
    expect(secondLine(row(), PLAYING, BANK, ended, null, NOW).text).toBe('The Bank of Aabahran');
  });

  it('shows no health while the game hides your vitals', () => {
    const hidden = run(latch.runs[0].vitals.slice(0, 4));
    expect(hidden?.hidden).toBe(true);
    expect(secondLine(row(), PLAYING, BANK, null, hidden, NOW)).toMatchObject({
      text: 'The Bank of Aabahran',
      health: null,
      low: false,
    });
  });

  it('starts the line of a session you named with its character', () => {
    expect(secondLine(row({ name: 'Build' }), PLAYING, BANK, null, health(846, 900), NOW)).toEqual({
      who: 'Orla',
      text: 'The Bank of Aabahran',
      health: 94,
      low: false,
    });
    // A name that is the character adds nothing.
    expect(secondLine(row({ name: 'Orla' }), PLAYING, BANK, null, null, NOW).who).toBeNull();
  });

  it('says what happened while the session does not play', () => {
    const text = (fields: Partial<SessionRowState>, of = row()) =>
      secondLine(of, state(fields), BANK, null, health(846, 900), NOW);
    const login = row({ name: 'Errands', character: null, port: 1848 });
    expect(text({ link: 'live' }, login)).toEqual({
      who: null,
      text: 'Waiting for your login',
      health: null,
      low: false,
    });
    expect(text({ link: 'dialing' }).text).toBe('Connecting…');
    expect(text({ link: 'dialing', redialing: true, try: 3, tries: 8 }).text).toBe(
      'Reconnecting, try 3 of 8',
    );
    expect(text({ link: 'failed', downAt: NOW - 4 * 60_000 - 5_000 }).text).toBe(
      'Dropped 4 min ago',
    );
    expect(text({ link: 'failed', downAt: NOW - 20_000 }).text).toBe('Dropped just now');
    expect(text({ link: 'failed', downAt: NOW - 1_000, refused: true }).text).toBe(
      'Couldn’t connect',
    );
    const off = text({ link: 'down' });
    expect(off).toEqual({ who: null, text: 'The Forsaken Lands', health: null, low: false });
    expect(text({}, row({ connected: false, host: null, port: null })).text).toBeNull();
  });
});

describe('how long a span lasts', () => {
  it('reads minutes, then hours and minutes', () => {
    expect(howLong(59_000)).toBeNull();
    expect(howLong(4 * 60_000)).toBe('4 min');
    expect(howLong(72 * 60_000)).toBe('1 h 12 min');
    expect(howLong(120 * 60_000)).toBe('2 h');
  });
});

/** The vitals after each step of a run of the latch fixture. */
function run(steps: { hp: number; maxhp: number; hidden: boolean }[]): Vitals | null {
  let vitals: Vitals | null = null;
  for (const v of steps)
    vitals = nextVitals(vitals, { ...FULL, hp: v.hp, maxhp: v.maxhp }, v.hidden);
  return vitals;
}
