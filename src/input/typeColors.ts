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

/** The word that decides a line: where it starts and ends, and the #
 *  command it names. A # line reads its name the way the dispatcher
 *  does, past any space after the #, and `#walk` may end at a `;`. */
function leadWord(line: string): { start: number; end: number; command: string | null } {
  const start = line.length - line.trimStart().length;
  const first = line.slice(start).split(/\s/, 1)[0];
  if (!first.startsWith('#')) return { start, end: start + first.length, command: null };
  const rest = line.slice(start + 1);
  const at = start + 1 + rest.length - rest.trimStart().length;
  const body = line.slice(at);
  const walk = /^walk(?=$|[;\s])/.test(body);
  const command = walk ? 'walk' : body.split(/\s/, 1)[0];
  return { start, end: at + command.length, command };
}

/** What `line` is, judged by its first word, or null for a plain line.
 *  A # line is a command before anything else, as the dispatcher reads
 *  it. An alias matches its name exactly, case and all, as it expands. */
export function kindOf(line: string, words: KnownWords): TypeKind | null {
  const { start, end, command } = leadWord(line);
  if (command !== null) return words.commands.includes(command) ? 'hash' : 'unknown';
  if (start === end) return null;
  if (words.aliases.includes(line.slice(start, end))) return 'alias';
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
    const { start, end } = leadWord(line);
    push(line.slice(0, start), null);
    push(line.slice(start, end), kind);
    push(line.slice(end), null);
  });
  return spans;
}
