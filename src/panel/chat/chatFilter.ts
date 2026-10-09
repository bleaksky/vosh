import { listJoin } from '../../lib/text';
import type { ChatLine } from '../../stores/gmcp/chatStore';
import { GAME_CHANNEL_SLOTS } from '../../theme/gameChannels';
import { isLeaf, type PaneLeaf, type PaneNode, type PaneRef } from '../paneLayout';

// What a Chat pane shows. A lone Chat pane shows All, or the channels
// you check. With two or more, each shows the channels you check in it,
// or Everything else, the lines on channels no other Chat pane checks.
// One pane at a time shows Everything else, and All is not offered.
//
// The filter lives in the leaf's props. A checked channel is
// channel = "tell". Two or more are channel = the first, sorted, beside
// channels = all of them joined with commas, so A3 and 0.8.1, which
// read channel alone, show the first. Everything else is rest = "1"
// beside an empty channel, which those builds read as All. A pane with
// neither reads as All alone, as Everything else when it is the first
// such pane and no pane says rest, and as no channels otherwise.

export type ChatFilter =
  | { kind: 'all' }
  | { kind: 'rest' }
  | { kind: 'channels'; channels: string[] }
  | { kind: 'none' };

export const EVERYTHING_ELSE = 'Everything else';

/** The channels the props of a Chat pane check, sorted, or none. A
 *  channels list counts only while its first entry is channel, so a
 *  pick an older build made, which writes channel alone, wins. */
export function checkedChannels(ref: PaneRef): string[] {
  const channel = ref.props.channel ?? '';
  if (channel === '') return [];
  const list = (ref.props.channels ?? '')
    .split(',')
    .map((c) => c.trim())
    .filter((c) => c !== '');
  return list[0] === channel ? [...new Set(list)].sort() : [channel];
}

/** The filter the props of a Chat pane name on their own: channels,
 *  Everything else, or All. */
export function chatFilterOf(ref: PaneRef): ChatFilter {
  const channels = checkedChannels(ref);
  if (channels.length > 0) return { kind: 'channels', channels };
  return ref.props.rest === '1' ? { kind: 'rest' } : { kind: 'all' };
}

/** The props patch that stores `filter`. No channels stores as a pane
 *  that is not Everything else, which reads as no channels while
 *  another pane shows Everything else. */
export function chatFilterProps(filter: ChatFilter): Record<string, string> {
  const channels = filter.kind === 'channels' ? [...new Set(filter.channels)].sort() : [];
  return {
    channel: channels[0] ?? '',
    channels: channels.length > 1 ? channels.join(',') : '',
    rest: filter.kind === 'rest' ? '1' : '',
  };
}

/** Every Chat leaf of the tree, in reading order. */
export function chatLeaves(node: PaneNode | null): PaneLeaf[] {
  if (node === null) return [];
  if (isLeaf(node)) return node.pane === 'chat' ? [node] : [];
  return node.children.flatMap(chatLeaves);
}

/** The Chat pane that shows Everything else while two or more show:
 *  the first that says rest, else the first with no channels checked,
 *  else none. */
export function restPaneId(tree: PaneNode | null): string | null {
  const chats = chatLeaves(tree);
  if (chats.length < 2) return null;
  const rest = chats.find((leaf) => chatFilterOf(leaf).kind === 'rest');
  const open = chats.find((leaf) => chatFilterOf(leaf).kind === 'all');
  return (rest ?? open)?.id ?? null;
}

/** What Chat pane `leaf` shows in `tree`. A pane the tree does not
 *  hold reads as a lone pane. */
export function chatFilterIn(tree: PaneNode | null, leaf: PaneLeaf): ChatFilter {
  const chats = chatLeaves(tree);
  const stored = chatFilterOf(leaf);
  if (chats.length < 2 || !chats.some((c) => c.id === leaf.id)) {
    return stored.kind === 'channels' ? stored : { kind: 'all' };
  }
  if (stored.kind === 'channels') return stored;
  return restPaneId(tree) === leaf.id ? { kind: 'rest' } : { kind: 'none' };
}

/** The channels a filter menu offers. Every channel the game has, so
 *  you can pick one before anyone talks on it, then the ones `heard`
 *  and the ones the pane checks, such as a routed pane, sorted. */
export function menuChannels(heard: readonly string[], checked: readonly string[]): string[] {
  return [...new Set([...GAME_CHANNEL_SLOTS.keys(), ...heard, ...checked])].sort();
}

/** A channel as the pane names it. The game sends its channels in
 *  lower case, tell and gtell, so the name takes a capital first letter
 *  beside Everything else. The filter keeps the game's name. */
export function channelName(channel: string): string {
  return channel.charAt(0).toUpperCase() + channel.slice(1);
}

/** What the filter reads as in the pane's header and its label. */
export function chatFilterLabel(filter: ChatFilter): string {
  switch (filter.kind) {
    case 'channels':
      return filter.channels.map(channelName).join(', ');
    case 'rest':
      return EVERYTHING_ELSE;
    case 'none':
      return 'Pick channels';
    default:
      return 'All';
  }
}

/** What an empty pane says will show in it. Everything else names the
 *  channels it leaves to the other panes, `owned`. */
export function chatEmptyText(filter: ChatFilter, owned: ReadonlySet<string>): string {
  switch (filter.kind) {
    case 'channels':
      return `${listJoin(filter.channels.map(channelName))} messages show up here as they come in.`;
    case 'none':
      return 'Pick the channels this pane shows from the menu up top.';
    case 'all':
      return 'Chat appears when someone talks on a channel.';
    default: {
      const names = [...owned].sort().map(channelName);
      const but = names.length > 0 ? ` but ${listJoin(names)}` : '';
      return `Messages on every channel${but} show up here as they come in.`;
    }
  }
}

/** The channels the Chat panes other than leaf `id` check. */
export function ownPaneChannels(tree: PaneNode | null, id: string): Set<string> {
  const out = new Set<string>();
  for (const leaf of chatLeaves(tree)) {
    const filter = chatFilterIn(tree, leaf);
    if (leaf.id !== id && filter.kind === 'channels') filter.channels.forEach((c) => out.add(c));
  }
  return out;
}

/** `filter` with `channel` checked or unchecked. Unchecking the last
 *  channel leaves a lone pane on All, and with other panes leaves
 *  Everything else when no other pane shows it, `restFree`. */
export function toggleChannel(
  filter: ChatFilter,
  channel: string,
  lone: boolean,
  restFree: boolean,
): ChatFilter {
  const now = filter.kind === 'channels' ? filter.channels : [];
  const next = now.includes(channel) ? now.filter((c) => c !== channel) : [...now, channel].sort();
  if (next.length > 0) return { kind: 'channels', channels: next };
  if (lone) return { kind: 'all' };
  return restFree ? { kind: 'rest' } : { kind: 'none' };
}

/** The lines a pane with `filter` shows, where `owned` holds the
 *  channels other Chat panes check. */
export function chatLinesFor(
  lines: ChatLine[],
  filter: ChatFilter,
  owned: ReadonlySet<string>,
): ChatLine[] {
  switch (filter.kind) {
    case 'all':
      return lines;
    case 'channels':
      return lines.filter((l) => filter.channels.includes(l.pane));
    case 'none':
      return [];
    default:
      return lines.filter((l) => !owned.has(l.pane));
  }
}
