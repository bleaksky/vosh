import { describe, expect, it, vi } from 'vitest';
import { parseCommChannel, type ChatLine } from '../../stores/gmcp/chatStore';
import { aabahranChatPacket } from '../../test/aabahranGmcp';
import { sanitize, type PaneSplit } from '../paneLayout';
import {
  chatFilterLabel,
  chatFilterOf,
  chatLeaves,
  chatLinesFor,
  ownPaneChannels,
} from './chatFilter';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/api/event', () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
}));

const chat = (props: Record<string, string>) => ({ pane: 'chat' as const, props });

const packet = (name: string): ChatLine => {
  const line = parseCommChannel(aabahranChatPacket(name).data as never, 0);
  if (!line) throw new Error(`no line in ${name}`);
  return line;
};

// The b4 board: Affects and Group, then Chat on tell over Chat on
// Everything else.
const B4: PaneSplit = sanitize({
  split: 'column',
  children: [
    { id: 'affects', pane: 'affects', props: {} },
    { id: 'group', pane: 'group', props: {} },
    { id: 'chat', ...chat({ channel: 'tell' }) },
    { id: 'chat-2', ...chat({ rest: '1' }) },
  ],
});

describe('chatFilterOf', () => {
  it('reads a channel, Everything else, or All from the props', () => {
    expect(chatFilterOf(chat({ channel: 'tell' }))).toEqual({ kind: 'channel', channel: 'tell' });
    expect(chatFilterOf(chat({ rest: '1' }))).toEqual({ kind: 'rest' });
    expect(chatFilterOf(chat({}))).toEqual({ kind: 'all' });
  });

  it('lets a named channel win over rest', () => {
    expect(chatFilterOf(chat({ channel: 'say', rest: '1' }))).toEqual({
      kind: 'channel',
      channel: 'say',
    });
  });

  it('reads as the channel, Everything else, or All', () => {
    expect(chatFilterLabel({ kind: 'channel', channel: 'tell' })).toBe('tell');
    expect(chatFilterLabel({ kind: 'rest' })).toBe('Everything else');
    expect(chatFilterLabel({ kind: 'all' })).toBe('All');
  });
});

describe('ownPaneChannels', () => {
  it('lists the channels the other Chat panes show on their own', () => {
    expect(chatLeaves(B4).map((l) => l.id)).toEqual(['chat', 'chat-2']);
    expect([...ownPaneChannels(B4, 'chat-2')]).toEqual(['tell']);
    expect([...ownPaneChannels(B4, 'chat')]).toEqual([]);
    expect([...ownPaneChannels(null, 'chat')]).toEqual([]);
  });
});

describe('chatLinesFor', () => {
  const lines = ['tell.gmcp', 'say.gmcp', 'yell.gmcp', 'tell-foreign.gmcp'].map(packet);
  const panes = (out: ChatLine[]) => out.map((l) => l.pane);

  it('shows every line on All and one channel on a channel', () => {
    expect(panes(chatLinesFor(lines, { kind: 'all' }, new Set(['tell'])))).toEqual([
      'tell',
      'say',
      'yell',
      'tell',
    ]);
    expect(panes(chatLinesFor(lines, { kind: 'channel', channel: 'tell' }, new Set()))).toEqual([
      'tell',
      'tell',
    ]);
  });

  it('shows on Everything else each line no other pane shows on its own', () => {
    expect(panes(chatLinesFor(lines, { kind: 'rest' }, new Set(['tell'])))).toEqual([
      'say',
      'yell',
    ]);
    expect(panes(chatLinesFor(lines, { kind: 'rest' }, new Set()))).toEqual(panes(lines));
  });
});
