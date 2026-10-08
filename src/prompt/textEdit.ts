// Edit as text in the prompt card: your design byte for byte, each
// token colored by what it is, the token under the caret marked with
// the part it draws, names Vosh has no value for underlined, and the
// token rows that add a token at the caret. Pure, so PromptText.tsx
// stays about the field.

import type { PromptToken } from '../ipc/promptDesign';

/** The token rows under the field. */
export const TOKEN_ROWS: readonly { label: string; tokens: readonly string[] }[] = [
  { label: 'Forms', tokens: ['%pct_hp%%', '%hp_bar:10', '%hp/%{maxhp}'] },
  { label: 'Color', tokens: ['%c_hp', '%c_4', '%{c:#80c8ff}', '%c_default', '%c_reset'] },
  { label: 'Style', tokens: ['%s_italic', '%s_bold', '%s_underline', '%s_off'] },
  { label: 'Layout', tokens: ['%nl', '%{if:fight}', '%{end}'] },
];

export const TEXT_HELP =
  'Click a token to add it at the cursor. %c_default goes back to the normal color and keeps your style. Vosh rewrites this text when you change a piece.';

/** What a name Vosh has no value for says on hover. */
export function unknownTitle(name: string | null): string {
  return name ? `Vosh has no value called ${name}.` : 'Vosh does not know this code.';
}

/** How a token draws in the field: values in the accent color, codes and
 *  layout in the tertiary color, text in the text color, and a name Vosh
 *  does not know with a dotted warn underline. */
export function tokenTone(token: PromptToken): 'value' | 'code' | 'text' | 'unknown' {
  if (!token.known) return 'unknown';
  switch (token.kind) {
    case 'value':
    case 'raw':
      return 'value';
    case 'text':
      return 'text';
    default:
      return 'code';
  }
}

/** The part the token at the caret draws: the token that ends at the
 *  caret, else the one it sits in or starts at. Null in an empty design. */
export function pieceAtCaret(tokens: readonly PromptToken[], caret: number): number | null {
  const ending = tokens.find((t) => t.end === caret && t.start < caret);
  if (ending) return ending.piece;
  const inside = tokens.find((t) => t.start <= caret && caret < t.end);
  return inside?.piece ?? tokens.at(-1)?.piece ?? null;
}

/** The text the tokens of `piece` cover, start and end. */
export function pieceRange(
  tokens: readonly PromptToken[],
  piece: number | null,
): { start: number; end: number } | null {
  const own = tokens.filter((t) => t.piece === piece);
  if (own.length === 0) return null;
  return { start: own[0].start, end: own[own.length - 1].end };
}

/** `text` with `token` in place of the selection from `start` to `end`,
 *  and where the caret goes after it. */
export function insertAt(
  text: string,
  start: number,
  end: number,
  token: string,
): { text: string; caret: number } {
  const from = Math.max(0, Math.min(start, text.length));
  const to = Math.max(from, Math.min(end, text.length));
  return { text: text.slice(0, from) + token + text.slice(to), caret: from + token.length };
}

/** Pasted or typed text as a design takes it: one line, with no line
 *  ends, since a line break is %nl. */
export function oneLine(text: string): string {
  return text.replace(/\r?\n|\r/g, '');
}

const escapeHtml = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/** The field's markup: each token in a span by its tone, a break chance
 *  between tokens and none inside one, and the tokens of `marked` inside
 *  one span that carries the selection token. Text the tokens leave out,
 *  which only a design being typed has, follows as text. */
export function fieldHtml(
  text: string,
  tokens: readonly PromptToken[],
  marked: number | null,
): string {
  let out = '';
  let open = false;
  let at = 0;
  tokens.forEach((token, i) => {
    const mark = token.piece === marked;
    if (open && !mark) {
      out += '</span>';
      open = false;
    }
    if (i > 0) out += '<wbr>';
    if (token.start > at) out += escapeHtml(text.slice(at, token.start));
    if (mark && !open) {
      out += '<span class="pc-text-mark">';
      open = true;
    }
    const tone = tokenTone(token);
    const title = tone === 'unknown' ? ` title="${escapeHtml(unknownTitle(token.name))}"` : '';
    out += `<span class="pc-tok is-${tone}"${title}>${escapeHtml(text.slice(token.start, token.end))}</span>`;
    at = token.end;
  });
  if (open) out += '</span>';
  if (at < text.length) out += escapeHtml(text.slice(at));
  return out;
}

/** Where the field's caret starts: where you left it, kept by the card
 *  while Insert value… replaced the field, held inside `template`, or at
 *  its end. */
export function keptCaret(
  template: string,
  kept: { start: number; end: number } | null,
): { start: number; end: number } {
  if (!kept) return { start: template.length, end: template.length };
  const end = Math.min(kept.end, template.length);
  return { start: Math.min(kept.start, end), end };
}
