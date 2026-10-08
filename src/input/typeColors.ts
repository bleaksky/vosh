// Coloring as you type. The first word of a line decides its color, and
// only what Vosh knows counts: your aliases, the # commands Vosh runs
// and the chat test spell check reads. Game commands stay plain.

import type { KnownWords } from '../ipc/input';
import { looksLikeChat } from './chatLine';

/** What a line you type is to Vosh. */
export type TypeKind = 'alias' | 'hash' | 'unknown' | 'chat';

/** A run of the command line in one color, null for plain. */
export interface TypeSpan {
  text: string;
  kind: TypeKind | null;
}

/** What `line` is, judged by its first word, or null for a plain line.
 *  An alias matches its name exactly, case and all, as it expands. */
export function kindOf(line: string, words: KnownWords): TypeKind | null {
  const first = line.trimStart().split(/\s/, 1)[0];
  if (first === '') return null;
  if (words.aliases.includes(first)) return 'alias';
  if (first.startsWith('#')) return words.commands.includes(first.slice(1)) ? 'hash' : 'unknown';
  return looksLikeChat(line) ? 'chat' : null;
}

/** The command line as colored runs. Each line of a compose is judged on
 *  its own, since each goes out as its own command. A chat line colors
 *  whole, and the rest color their first word only. */
export function typeSpans(value: string, words: KnownWords): TypeSpan[] {
  const spans: TypeSpan[] = [];
  const push = (text: string, kind: TypeKind | null) => {
    if (text === '') return;
    const last = spans[spans.length - 1];
    if (last && last.kind === kind) last.text += text;
    else spans.push({ text, kind });
  };
  value.split('\n').forEach((line, i) => {
    if (i > 0) push('\n', null);
    const kind = kindOf(line, words);
    if (kind === null || kind === 'chat') {
      push(line, kind);
      return;
    }
    const start = line.length - line.trimStart().length;
    const end = start + line.slice(start).split(/\s/, 1)[0].length;
    push(line.slice(0, start), null);
    push(line.slice(start, end), kind);
    push(line.slice(end), null);
  });
  return spans;
}
