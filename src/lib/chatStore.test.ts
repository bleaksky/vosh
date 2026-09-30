import { listen } from '@tauri-apps/api/event';
import { describe, expect, it, vi } from 'vitest';
import { aabahranChatFixtureNames, aabahranChatPacket } from '../test/aabahranGmcp';
import { getChatLines, parseCommChannel, parseRoutedLine, type ChatLine } from './chatStore';
import type { RoutedPayload } from './session';

// The store reaches the Tauri bridge when it starts. The parsers under
// test never do.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const TS = 1_790_000_000_000;

function read(name: string): ChatLine | null {
  const { package: pkg, data } = aabahranChatPacket(name);
  expect(pkg, name).toBe('Comm.Channel');
  return parseCommChannel(data, TS);
}

describe('parseCommChannel', () => {
  it('reads every Aabahran chat packet', () => {
    const names = aabahranChatFixtureNames();
    expect(names.length).toBeGreaterThan(0);
    for (const name of names) expect(read(name), name).not.toBeNull();
  });

  it('keeps the speaker apart from the message', () => {
    expect(read('say.gmcp')).toEqual({
      pane: 'say',
      speaker: 'Tarvik',
      text: 'grabbing my bank box, back soon',
      language: 'common',
      understood: true,
      direction: null,
      ts: TS,
    });
  });

  it('keeps a speaker of several words whole', () => {
    expect(read('yell.gmcp')?.speaker).toBe('a Blackwatch villager');
    expect(read('gtell-disguised.gmcp')?.speaker).toBe('{Tarvik} a shadow');
  });

  it('reads the direction of a tell you receive', () => {
    expect(read('tell.gmcp')).toMatchObject({
      pane: 'tell',
      speaker: 'Selune',
      text: 'are you still at the bank?',
      direction: 'received',
    });
  });

  it('reads the language of a message you did not understand', () => {
    expect(read('tell-foreign.gmcp')).toMatchObject({
      language: 'foreign',
      understood: false,
      text: 'ith vaen dorlas mir',
    });
  });

  it('leaves language and direction empty on channels that carry none', () => {
    for (const name of ['cabal.gmcp', 'immortal.gmcp']) {
      expect(read(name), name).toMatchObject({
        language: null,
        understood: null,
        direction: null,
      });
    }
  });

  it('strips color codes from the speaker and the message', () => {
    expect(
      parseCommChannel(
        { channel: 'say', speaker: '\x1b[1mTarvik\x1b[0m', text: '\x1b[33mhi\x1b[0m' },
        TS,
      ),
    ).toMatchObject({ speaker: 'Tarvik', text: 'hi' });
  });

  it('reads the field names other games use', () => {
    expect(parseCommChannel({ chan: 'gossip', talker: 'Bob', msg: 'hello' }, TS)).toMatchObject({
      pane: 'gossip',
      speaker: 'Bob',
      text: 'hello',
    });
    expect(parseCommChannel({ message: 'hello' }, TS)).toMatchObject({
      pane: 'chat',
      speaker: null,
      text: 'hello',
    });
  });

  it('drops a packet with no message', () => {
    expect(parseCommChannel({ channel: 'say', speaker: 'Tarvik', text: '' }, TS)).toBeNull();
    expect(parseCommChannel(null, TS)).toBeNull();
    expect(parseCommChannel('say hi', TS)).toBeNull();
  });

  it('reads only the directions the protocol names', () => {
    expect(
      parseCommChannel({ channel: 'tell', speaker: 'Selune', text: 'x', direction: 'sent' }, TS)
        ?.direction,
    ).toBe('sent');
    expect(
      parseCommChannel({ channel: 'tell', speaker: 'Selune', text: 'x', direction: 'up' }, TS)
        ?.direction,
    ).toBeNull();
  });

  it('treats an empty speaker as none', () => {
    expect(parseCommChannel({ channel: 'say', speaker: '', text: 'x' }, TS)?.speaker).toBeNull();
  });
});

describe('parseRoutedLine', () => {
  it('keeps a routed line whole with no speaker', () => {
    expect(parseRoutedLine({ pane: 'loot', text: 'You get a gold coin.\r' }, TS)).toEqual({
      pane: 'loot',
      speaker: null,
      text: 'You get a gold coin.',
      language: null,
      understood: null,
      direction: null,
      ts: TS,
    });
  });

  it('strips color codes', () => {
    expect(parseRoutedLine({ pane: 'loot', text: '\x1b[33mgold\x1b[0m' }, TS)?.text).toBe('gold');
  });

  // The game sends no Comm.Channel for a tell you send, so a trigger that
  // routes the terminal line brings your side of the tell in.
  it('reads a tell you send off the terminal line', () => {
    expect(
      parseRoutedLine(
        {
          pane: 'tell',
          text: "You tell Selune '\x1b[32myes, inside. north from the square\x1b[0m'",
        },
        TS,
      ),
    ).toEqual({
      pane: 'tell',
      speaker: 'Selune',
      text: 'yes, inside. north from the square',
      language: 'common',
      understood: true,
      direction: 'sent',
      ts: TS,
    });
  });

  it('reads the language and a recipient of several words', () => {
    expect(
      parseRoutedLine({ pane: 'tell', text: "You tell a city guard in Tol'khan 'it's me'" }, TS),
    ).toMatchObject({
      speaker: 'a city guard',
      text: "it's me",
      language: "Tol'khan",
      direction: 'sent',
    });
  });

  it('reads a tell a telepath projects', () => {
    expect(
      parseRoutedLine({ pane: 'tell', text: "You project to Selune in Elvish 'omw'" }, TS),
    ).toMatchObject({ speaker: 'Selune', text: 'omw', language: 'Elvish', direction: 'sent' });
  });

  it('leaves other lines about tells whole', () => {
    for (const text of [
      "Selune tells you 'are you still at the bank?'",
      "You try to tell Selune in Elvish 'omw'",
    ]) {
      const line = parseRoutedLine({ pane: 'tell', text }, TS);
      expect(line?.speaker, text).toBeNull();
      expect(line?.text, text).toBe(text);
    }
  });

  // The game echoes a group tell you send back to you as a gtell packet
  // (act_comm.c do_gtell), so the terminal line would print it twice.
  it('drops the line for a group tell you send, whatever pane it routes to', () => {
    for (const pane of ['tell', 'gtell', 'loot']) {
      for (const text of [
        "You tell your group '\x1b[36mone tick, waiting on mana\x1b[0m'",
        "You tell your group in Elvish 'one tick'",
      ]) {
        expect(parseRoutedLine({ pane, text }, TS), `${pane}: ${text}`).toBeNull();
      }
    }
  });
});

describe('the chat store', () => {
  it('prints a tell you send once and leaves a routed group tell to its gtell packet', () => {
    expect(getChatLines()).toEqual([]);
    const call = vi.mocked(listen).mock.calls.find(([event]) => event === 'session://routed');
    if (!call) throw new Error('the store never listened for routed lines');
    const handler = call[1] as unknown as (event: { payload: RoutedPayload }) => void;
    const route = (text: string) => handler({ payload: { pane: 'tell', text } });

    route("You tell your group 'one tick, waiting on mana'");
    route("You tell Selune 'omw'");

    expect(getChatLines().map((l) => [l.pane, l.direction, l.speaker, l.text])).toEqual([
      ['tell', 'sent', 'Selune', 'omw'],
    ]);
  });
});
