import type { ChatLine } from '../../stores/gmcp/chatStore';
import { isLeaf, type PaneLeaf, type PaneNode, type PaneRef } from '../paneLayout';

// What a Chat pane shows: every line (All), one channel, or Everything
// else, the lines on channels no other Chat pane shows on its own. The
// filter lives in the leaf's props as channel, or as rest = "1" beside
// an empty channel, so an older build reads Everything else as All.

export type ChatFilter = { kind: 'all' } | { kind: 'rest' } | { kind: 'channel'; channel: string };

export const EVERYTHING_ELSE = 'Everything else';

/** The filter the props of a Chat pane name. */
export function chatFilterOf(ref: PaneRef): ChatFilter {
  const channel = ref.props.channel ?? '';
  if (channel !== '') return { kind: 'channel', channel };
  return ref.props.rest === '1' ? { kind: 'rest' } : { kind: 'all' };
}

/** What the filter reads as in the pane's header and its label. */
export function chatFilterLabel(filter: ChatFilter): string {
  if (filter.kind === 'channel') return filter.channel;
  return filter.kind === 'rest' ? EVERYTHING_ELSE : 'All';
}

/** Every Chat leaf of the tree, in reading order. */
export function chatLeaves(node: PaneNode | null): PaneLeaf[] {
  if (node === null) return [];
  if (isLeaf(node)) return node.pane === 'chat' ? [node] : [];
  return node.children.flatMap(chatLeaves);
}

/** The channels the Chat panes other than leaf `id` show on their own. */
export function ownPaneChannels(tree: PaneNode | null, id: string): Set<string> {
  const out = new Set<string>();
  for (const leaf of chatLeaves(tree)) {
    const filter = chatFilterOf(leaf);
    if (leaf.id !== id && filter.kind === 'channel') out.add(filter.channel);
  }
  return out;
}

/** The lines a pane with `filter` shows, where `owned` holds the
 *  channels other Chat panes show on their own. */
export function chatLinesFor(
  lines: ChatLine[],
  filter: ChatFilter,
  owned: ReadonlySet<string>,
): ChatLine[] {
  if (filter.kind === 'all') return lines;
  if (filter.kind === 'channel') return lines.filter((l) => l.pane === filter.channel);
  return lines.filter((l) => !owned.has(l.pane));
}
