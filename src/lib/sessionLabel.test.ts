import { describe, expect, it } from 'vitest';
import labelCases from '../../fixtures/session-labels/cases.json';
import { sessionLabel, typedName, type LabelSource } from './sessionLabel';

// A row reads the name you gave the session, else the character, else
// the world with its port, else New session. The port shows as quiet
// meta when it is not the world's own, on a named row too, and on a
// host Vosh does not know only while another open session shares that
// host.

const PLAY = 'play.theforsakenlands.com';

function session(id: number, fields: Partial<LabelSource> = {}): LabelSource {
  return { id, name: null, character: null, host: PLAY, port: 1848, ...fields };
}

describe('sessionLabel', () => {
  it('reads the character on the world it plays, its own port left out', () => {
    const tolliver = session(1, { character: 'Tolliver' });
    expect(sessionLabel(tolliver, [tolliver])).toEqual({
      name: 'Tolliver',
      who: 'Tolliver',
      place: 'The Forsaken Lands',
      meta: null,
      split: null,
    });
  });

  it('shows a port that is not the world own as meta', () => {
    const tolliver = session(1, { character: 'Tolliver' });
    const orla = session(2, { character: 'Orla', port: 1825 });
    expect(sessionLabel(orla, [tolliver, orla])).toEqual({
      name: 'Orla',
      who: 'Orla',
      place: 'The Forsaken Lands 1825',
      meta: '1825',
      split: null,
    });
  });

  it('reads the name you gave in place of the character, with the port still as meta', () => {
    const builder = session(2, { name: 'Builder', character: 'Tolliver', port: 1825 });
    expect(sessionLabel(builder, [builder])).toMatchObject({
      name: 'Builder',
      who: 'Builder',
      meta: '1825',
    });
    const home = session(1, { name: 'Main', character: 'Tolliver' });
    expect(sessionLabel(home, [home])).toMatchObject({ name: 'Main', meta: null });
  });

  it('reads the character again once the name is cleared', () => {
    const cleared = session(1, { name: '  ', character: 'Orla' });
    expect(sessionLabel(cleared, [cleared]).name).toBe('Orla');
  });

  it('names the world with its port before you log in, the port in the name and not the meta', () => {
    const build = session(3, { port: 1825 });
    expect(sessionLabel(build, [build])).toEqual({
      name: 'The Forsaken Lands 1825',
      who: null,
      place: 'The Forsaken Lands 1825',
      meta: null,
      split: { world: 'The Forsaken Lands', port: '1825' },
    });
    const play = session(4);
    expect(sessionLabel(play, [play])).toMatchObject({
      name: 'The Forsaken Lands',
      meta: null,
      split: null,
    });
  });

  it('reads New session before it has an address', () => {
    const fresh = session(5, { host: null, port: null });
    expect(sessionLabel(fresh, [fresh])).toEqual({
      name: 'New session',
      who: null,
      place: null,
      meta: null,
      split: null,
    });
  });

  it('shows the port on a host Vosh does not know only while another session shares it', () => {
    const alone = session(1, { character: 'Maren', host: 'mud.example.org', port: 4000 });
    expect(sessionLabel(alone, [alone])).toMatchObject({
      name: 'Maren',
      place: 'mud.example.org',
      meta: null,
    });
    const other = session(2, { character: 'Orla', host: 'MUD.example.org.', port: 4001 });
    expect(sessionLabel(alone, [alone, other])).toMatchObject({
      place: 'mud.example.org 4000',
      meta: '4000',
    });
    const elsewhere = session(3, { host: 'other.example.org', port: 4000 });
    expect(sessionLabel(alone, [alone, elsewhere]).meta).toBeNull();
  });

  it('keeps the port apart from a long world, so the world gives way first', () => {
    const long = 'a.rather.long.host.name.for.a.test.server.example.org';
    const first = session(1, { host: long, port: 7000 });
    const second = session(2, { host: long, port: 7001 });
    expect(sessionLabel(first, [first, second])).toMatchObject({
      name: `${long} 7000`,
      split: { world: long, port: '7000' },
    });
  });
});

describe('typedName', () => {
  it('changes nothing while the field still reads as the session did', () => {
    expect(typedName('Tolliver', 'Tolliver')).toBeUndefined();
    expect(typedName('  Tolliver ', 'Tolliver')).toBeUndefined();
  });

  it('gives the text you typed, without its outer spaces', () => {
    expect(typedName(' Builder  ', 'Tolliver')).toBe('Builder');
  });

  it('clears the name with a blank field', () => {
    expect(typedName('', 'Builder')).toBeNull();
    expect(typedName('   ', 'Builder')).toBeNull();
  });
});

describe('the name a session goes by', () => {
  // label_of in src-tauri/src/sessions.rs runs the same cases, so a
  // banner and the line another session prints name a session as its
  // row does.
  it.each(labelCases.cases)('$name', ({ session: named, others, label }) => {
    const row: LabelSource = { id: 1, ...named };
    const rows = [
      row,
      ...others.map((place, i) => ({ id: i + 2, name: null, character: null, ...place })),
    ];
    expect(sessionLabel(row, rows).name).toBe(label ?? 'New session');
  });
});
