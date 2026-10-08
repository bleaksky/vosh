import { classifyInline, inlinePieces } from './helpInline';
import { HELP_TOPICS, parseHelpBody, searchTopics, type HelpTopic } from './helpContent';

// Finding your way in the Help window (the approved Help boards): the
// search ranks topics and counts its matches, a long reference list
// gets an outline, and every link into Help names a topic or a search.

/** The topics of one section, in catalog order. */
export function sectionTopics(section: string, topics: HelpTopic[] = HELP_TOPICS): HelpTopic[] {
  return topics.filter((t) => t.section === section);
}

/** The text a topic draws, piece by piece: its title, then each run of
 *  plain text and each backticked span of its body, without the
 *  backticks. A match never crosses two pieces, so marks and counts
 *  read the same pieces. */
export function topicPieces(topic: HelpTopic): string[] {
  const pieces = [topic.title];
  for (const block of parseHelpBody(topic.body)) {
    if (block.kind === 'action') {
      pieces.push(block.label);
      continue;
    }
    const lines =
      block.kind === 'paragraph'
        ? [block.text]
        : block.kind === 'list'
          ? block.items
          : [...block.head, ...block.rows.flat()];
    for (const line of lines) {
      for (const piece of inlinePieces(line)) pieces.push(piece.text);
    }
  }
  return pieces;
}

/** The words of a search as the matcher reads them: in lower case,
 *  with a run of spaces read as one, since the help text sets one
 *  space between words. */
export function searchPhrase(query: string): string {
  return query.trim().replace(/\s+/g, ' ').toLowerCase();
}

/** Where `query` matches in `text`, ignoring case, as [start, end)
 *  ranges that never overlap. */
export function matchRanges(text: string, query: string): Array<[number, number]> {
  const q = searchPhrase(query);
  if (q.length === 0) return [];
  const hay = text.toLowerCase();
  const ranges: Array<[number, number]> = [];
  let at = hay.indexOf(q);
  while (at !== -1) {
    ranges.push([at, at + q.length]);
    at = hay.indexOf(q, at + q.length);
  }
  return ranges;
}

/** How many times `query` shows in the topic you would read. */
export function countMatches(topic: HelpTopic, query: string): number {
  return topicPieces(topic).reduce((n, piece) => n + matchRanges(piece, query).length, 0);
}

/** The topics a search finds, best first: a title that matches, then
 *  the most matches, then catalog order. */
export function rankTopics(query: string, topics: HelpTopic[] = HELP_TOPICS): HelpTopic[] {
  const q = searchPhrase(query);
  if (q.length === 0) return [];
  return searchTopics(q, topics)
    .map((topic, index) => ({
      topic,
      index,
      title: topic.title.toLowerCase().includes(q),
      count: countMatches(topic, query),
    }))
    .sort((a, b) => Number(b.title) - Number(a.title) || b.count - a.count || a.index - b.index)
    .map((r) => r.topic);
}

/** One row of On this page: the item it scrolls to and what it reads. */
export interface OutlineEntry {
  /** The block and the item within it, in the topic's blocks. */
  block: number;
  item: number;
  label: string;
}

/** The id of a list item an outline row scrolls to. */
export function helpItemId(block: number, item: number): string {
  return `hp-item-${block}-${item}`;
}

/** The fewest list items that earn an outline. */
const OUTLINE_MIN_ITEMS = 6;

/** The words a code names before its first argument, so
 *  `#prompt game {setting}` names `#prompt game`. */
function commandWords(code: string): string[] {
  const words: string[] = [];
  for (const word of code.split(/\s+/)) {
    if (word.length === 0) continue;
    if (/^[<{[(]/.test(word) || word.includes('|')) break;
    words.push(word);
  }
  return words.length > 0 ? words : [code];
}

/** The outline of a topic, or null when it has none. A list of six or
 *  more items that each open with a code earns one, a row per item.
 *  Each row takes as many words of its code as it needs to stay apart
 *  from the others, so `#tick` and `#tick warn` both show. */
export function outlineFor(topic: HelpTopic): OutlineEntry[] | null {
  const blocks = parseHelpBody(topic.body);
  for (let b = 0; b < blocks.length; b++) {
    const block = blocks[b];
    if (block.kind !== 'list' || block.items.length < OUTLINE_MIN_ITEMS) continue;
    const leads = block.items.map((item) => {
      const match = /^`([^`]+)`/.exec(item);
      return match && classifyInline(match[1]) === 'code' ? commandWords(match[1]) : null;
    });
    if (leads.some((lead) => lead === null)) continue;
    const words = leads as string[][];
    return words.map((own, i) => {
      let size = own.length;
      for (let k = 1; k <= own.length; k++) {
        const prefix = own.slice(0, k).join(' ');
        const shared = words.some(
          (other, j) => j !== i && other.length >= k && other.slice(0, k).join(' ') === prefix,
        );
        if (!shared) {
          size = k;
          break;
        }
      }
      return { block: b, item: i, label: own.slice(0, size).join(' ') };
    });
  }
  return null;
}

/** Where a link into Help lands: a topic, or a search for words. */
export type HelpTarget = { kind: 'topic'; topic: HelpTopic } | { kind: 'search'; query: string };

/** Read a link into Help: a topic id like `reference.prompt-codes`, a
 *  topic number like `9.3`, or words to search for. */
export function resolveHelpTarget(
  target: string,
  topics: HelpTopic[] = HELP_TOPICS,
): HelpTarget | null {
  const text = target.trim();
  if (text.length === 0) return null;
  const topic = topics.find((t) => t.id === text || t.number === text);
  return topic ? { kind: 'topic', topic } : { kind: 'search', query: text };
}

/** What a landing does to the window: the topic it shows, or null to
 *  show the best result of the search, the words in the search, the
 *  section that opens in the sidebar, and whether the caret goes into
 *  the search. A landing opens its section every time, even on the
 *  topic you are reading. A search that finds nothing names no
 *  section and leaves the sidebar as it is. */
export interface HelpLanding {
  topicId: string | null;
  query: string;
  section: string | null;
  focusSearch: boolean;
}

export function landingOf(target: HelpTarget, topics: HelpTopic[] = HELP_TOPICS): HelpLanding {
  if (target.kind === 'topic') {
    return {
      topicId: target.topic.id,
      query: '',
      section: target.topic.section,
      focusSearch: false,
    };
  }
  const best = rankTopics(target.query, topics)[0];
  return { topicId: null, query: target.query, section: best?.section ?? null, focusSearch: true };
}

/** What a key in the help search does. */
export type HelpSearchAction =
  | { kind: 'active'; index: number }
  | { kind: 'step'; by: 1 | -1 }
  | { kind: 'clear' }
  | { kind: 'blur' };

/** A key in the help search, as the Settings search reads it: Up and
 *  Down move through the results, and Escape clears the words, then
 *  leaves the field. Enter steps to the next match in the topic you
 *  read and Shift+Enter back, the way the find bar does, since the
 *  result you are on is open already. Null leaves the key to the
 *  field. */
export function helpSearchKey(
  key: string,
  shift: boolean,
  state: { active: number; results: number; query: string },
): HelpSearchAction | null {
  const { active, results, query } = state;
  if (key === 'ArrowDown' && results > 0) {
    return { kind: 'active', index: Math.min(active + 1, results - 1) };
  }
  if (key === 'ArrowUp' && results > 0) return { kind: 'active', index: Math.max(active - 1, 0) };
  if (key === 'Enter' && results > 0) return { kind: 'step', by: shift ? -1 : 1 };
  if (key === 'Escape') return query.length > 0 ? { kind: 'clear' } : { kind: 'blur' };
  return null;
}

/** Where focus sits when you press a key in Help: in the article
 *  column, in the search, on another control like a topic in the
 *  sidebar, or nowhere. */
export type HelpFocus = 'article' | 'field' | 'control' | 'none';

/** How a key moves the article: a page, a line, or to an end. */
export type HelpScroll =
  | { kind: 'page'; by: 1 | -1 }
  | { kind: 'line'; by: 1 | -1 }
  | { kind: 'edge'; to: 'top' | 'bottom' };

/** A key that scrolls the article from outside it, or null to leave
 *  the key alone. The article scrolls itself once it has focus. From
 *  the search only PageUp and PageDown scroll it, since the arrows move
 *  through the results and the rest edit the words. A control in the
 *  sidebar keeps Space to press it. With nothing focused, every
 *  scrolling key reaches the article. */
export function helpScrollKey(key: string, shift: boolean, focus: HelpFocus): HelpScroll | null {
  if (focus === 'article') return null;
  if (key === 'PageDown') return { kind: 'page', by: 1 };
  if (key === 'PageUp') return { kind: 'page', by: -1 };
  if (focus === 'field') return null;
  if (key === 'ArrowDown') return { kind: 'line', by: 1 };
  if (key === 'ArrowUp') return { kind: 'line', by: -1 };
  if (key === 'Home') return { kind: 'edge', to: 'top' };
  if (key === 'End') return { kind: 'edge', to: 'bottom' };
  if (focus === 'none' && key === ' ') return { kind: 'page', by: shift ? -1 : 1 };
  return null;
}
