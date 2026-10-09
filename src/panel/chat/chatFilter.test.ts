import { describe, expect, it, vi } from 'vitest';
import { parseCommChannel, type ChatLine } from '../../stores/gmcp/chatStore';
import { aabahranChatPacket } from '../../test/aabahranGmcp';
import { sanitize, type PaneSplit } from '../paneLayout';
import {
  channelName,
  chatEmptyText,
  chatFilterIn,
  chatFilterLabel,
  chatFilterOf,
  menuChannels,
  chatFilterProps,
  checkedChannels,
  restPaneId,
  toggleChannel,
  type ChatFilter,
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
  it('reads channels, Everything else, or All from the props', () => {
    expect(chatFilterOf(chat({ channel: 'tell' }))).toEqual({
      kind: 'channels',
      channels: ['tell'],
    });
    expect(chatFilterOf(chat({ channel: 'gtell', channels: 'gtell,tell' }))).toEqual({
      kind: 'channels',
      channels: ['gtell', 'tell'],
    });
    expect(chatFilterOf(chat({ rest: '1' }))).toEqual({ kind: 'rest' });
    expect(chatFilterOf(chat({}))).toEqual({ kind: 'all' });
  });

  it('lets a named channel win over rest', () => {
    expect(chatFilterOf(chat({ channel: 'say', rest: '1' }))).toEqual({
      kind: 'channels',
      channels: ['say'],
    });
  });

  it('reads the channel alone once an older build picks another', () => {
    // A3 and 0.8.1 write channel and leave channels as it was.
    expect(checkedChannels(chat({ channel: 'say', channels: 'gtell,tell' }))).toEqual(['say']);
  });

  it('stores channels as the first beside the list, so older builds show the first', () => {
    expect(chatFilterProps({ kind: 'channels', channels: ['tell', 'gtell'] })).toEqual({
      channel: 'gtell',
      channels: 'gtell,tell',
      rest: '',
    });
    expect(chatFilterProps({ kind: 'channels', channels: ['tell'] })).toEqual({
      channel: 'tell',
      channels: '',
      rest: '',
    });
    expect(chatFilterProps({ kind: 'rest' })).toEqual({ channel: '', channels: '', rest: '1' });
    expect(chatFilterProps({ kind: 'all' })).toEqual({ channel: '', channels: '', rest: '' });
  });

  it('reads as the channels, Everything else, All, or a prompt to pick', () => {
    expect(chatFilterLabel({ kind: 'channels', channels: ['tell'] })).toBe('Tell');
    expect(chatFilterLabel({ kind: 'channels', channels: ['gtell', 'tell'] })).toBe('Gtell, Tell');
    expect(chatFilterLabel({ kind: 'rest' })).toBe('Everything else');
    expect(chatFilterLabel({ kind: 'all' })).toBe('All');
    expect(chatFilterLabel({ kind: 'none' })).toBe('Pick channels');
  });
});

describe('chatFilterIn', () => {
  const tree = (...props: Record<string, string>[]): PaneSplit =>
    sanitize({
      split: 'column',
      children: props.map((p, i) => ({ id: i === 0 ? 'chat' : `chat-${i + 1}`, ...chat(p) })),
    });
  const read = (t: PaneSplit) =>
    chatLeaves(t).map((leaf) => chatFilterLabel(chatFilterIn(t, leaf)));

  it('shows All on a lone pane, whatever older props say', () => {
    expect(read(tree({}))).toEqual(['All']);
    expect(read(tree({ rest: '1' }))).toEqual(['All']);
    expect(read(tree({ channel: 'tell' }))).toEqual(['Tell']);
  });

  it('reads an All pane as Everything else beside another pane', () => {
    expect(read(tree({}, { channel: 'tell' }))).toEqual(['Everything else', 'Tell']);
  });

  it('lets one pane show Everything else, the one that says so first', () => {
    expect(read(tree({}, { rest: '1' }, { rest: '1' }))).toEqual([
      'Pick channels',
      'Everything else',
      'Pick channels',
    ]);
    expect(read(tree({}, {}))).toEqual(['Everything else', 'Pick channels']);
    expect(restPaneId(tree({}, { rest: '1' }))).toBe('chat-2');
    expect(restPaneId(tree({ rest: '1' }))).toBeNull();
  });
});

describe('toggleChannel', () => {
  it('checks and unchecks channels, sorted', () => {
    const one = toggleChannel({ kind: 'channels', channels: ['tell'] }, 'gtell', false, false);
    expect(one).toEqual({ kind: 'channels', channels: ['gtell', 'tell'] });
    expect(toggleChannel(one, 'gtell', false, false)).toEqual({
      kind: 'channels',
      channels: ['tell'],
    });
    expect(toggleChannel({ kind: 'rest' }, 'say', false, true)).toEqual({
      kind: 'channels',
      channels: ['say'],
    });
  });

  it('leaves All, Everything else or no channels as the last one goes', () => {
    const tell: ChatFilter = { kind: 'channels', channels: ['tell'] };
    expect(toggleChannel(tell, 'tell', true, true)).toEqual({ kind: 'all' });
    expect(toggleChannel(tell, 'tell', false, true)).toEqual({ kind: 'rest' });
    expect(toggleChannel(tell, 'tell', false, false)).toEqual({ kind: 'none' });
  });
});

describe('channelName', () => {
  it('puts the first letter of the game name up and keeps the rest', () => {
    expect(channelName('tell')).toBe('Tell');
    expect(channelName('gtell')).toBe('Gtell');
    expect(channelName('newbie')).toBe('Newbie');
    expect(channelName('Tells')).toBe('Tells');
    expect(channelName('')).toBe('');
  });
});

describe('chatEmptyText', () => {
  it('names the channel a pane shows', () => {
    expect(chatEmptyText({ kind: 'channels', channels: ['tell'] }, new Set())).toBe(
      'Tell messages show up here as they come in.',
    );
    expect(chatEmptyText({ kind: 'channels', channels: ['gtell', 'say', 'tell'] }, new Set())).toBe(
      'Gtell, Say, and Tell messages show up here as they come in.',
    );
    expect(chatEmptyText({ kind: 'none' }, new Set())).toBe(
      'Pick the channels this pane shows from the menu up top.',
    );
  });

  it('names the channels Everything else leaves to their own panes', () => {
    expect(chatEmptyText({ kind: 'rest' }, new Set())).toBe(
      'Messages on every channel show up here as they come in.',
    );
    expect(chatEmptyText({ kind: 'rest' }, new Set(['tell']))).toBe(
      'Messages on every channel but Tell show up here as they come in.',
    );
    expect(chatEmptyText({ kind: 'rest' }, new Set(['tell', 'gtell', 'say']))).toBe(
      'Messages on every channel but Gtell, Say, and Tell show up here as they come in.',
    );
  });

  it('keeps the All line', () => {
    expect(chatEmptyText({ kind: 'all' }, new Set(['tell']))).toBe(
      'Chat appears when someone talks on a channel.',
    );
  });
});

describe('ownPaneChannels', () => {
  it('leaves out a pane on All, which shows no channel on its own', () => {
    const three: PaneSplit = sanitize({
      split: 'column',
      children: [
        { id: 'chat', ...chat({ rest: '1' }) },
        { id: 'chat-2', ...chat({ channel: 'tell' }) },
        { id: 'chat-3', ...chat({}) },
      ],
    });
    expect([...ownPaneChannels(three, 'chat')]).toEqual(['tell']);
    const withAll: PaneSplit = sanitize({
      split: 'column',
      children: [
        { id: 'chat', ...chat({ rest: '1' }) },
        { id: 'chat-3', ...chat({}) },
      ],
    });
    expect([...ownPaneChannels(withAll, 'chat')]).toEqual([]);
  });

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
    expect(panes(chatLinesFor(lines, { kind: 'channels', channels: ['tell'] }, new Set()))).toEqual(
      ['tell', 'tell'],
    );
    expect(
      panes(chatLinesFor(lines, { kind: 'channels', channels: ['say', 'tell'] }, new Set())),
    ).toEqual(['tell', 'say', 'tell']);
    expect(panes(chatLinesFor(lines, { kind: 'none' }, new Set()))).toEqual([]);
  });

  it('shows on Everything else each line no other pane shows on its own', () => {
    expect(panes(chatLinesFor(lines, { kind: 'rest' }, new Set(['tell'])))).toEqual([
      'say',
      'yell',
    ]);
    expect(panes(chatLinesFor(lines, { kind: 'rest' }, new Set()))).toEqual(panes(lines));
  });
});

describe('menuChannels', () => {
  it('offers every game channel before anyone talks, with heard and checked ones', () => {
    const none = menuChannels([], []);
    expect(none).toContain('tell');
    expect(none).toContain('gtell');
    expect(none).toContain('say');
    const more = menuChannels(['ooc'], ['routed']);
    expect(more).toContain('ooc');
    expect(more).toContain('routed');
    expect(more).toEqual([...more].sort());
  });
});
