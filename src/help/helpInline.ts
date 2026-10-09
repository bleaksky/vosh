// How Help draws a backticked span. MUD text, commands, codes and files
// are mono chips. A label you see in Vosh, like Edit as text, is SF
// 600. A key you press, like Shift+Enter, is a row of keycaps.

export type InlineKind = 'code' | 'label' | 'key';

/** Keys a combo can hold besides a modifier. Arrow keys go by their
 *  full names, ArrowUp and the rest, since Up and Down alone are also
 *  labels in Vosh (Tick counts Up). */
const NAMED_KEYS = new Set([
  'Enter',
  'Return',
  'Tab',
  'Esc',
  'Escape',
  'Space',
  'Backspace',
  'Delete',
  'Home',
  'End',
  'PageUp',
  'PageDown',
  'ArrowUp',
  'ArrowDown',
  'ArrowLeft',
  'ArrowRight',
]);

const MODIFIERS = new Set(['Cmd', 'Ctrl', 'Shift', 'Alt', 'Option', 'Fn']);

/** The arrows, which only follow a modifier when written short. */
const ARROWS = new Set(['Up', 'Down', 'Left', 'Right']);

function isNamedKey(part: string): boolean {
  return NAMED_KEYS.has(part) || /^F([1-9]|1[0-9]|2[0-4])$/.test(part) || /^Numpad\w+$/.test(part);
}

/** The keys of a combo like `Shift+Enter`, or null when `span` is no
 *  key. A modifier alone, like `Shift`, is a key. A letter or an arrow
 *  counts only after a modifier, as in `Cmd+F` or `Fn+Up`, and a glyph
 *  shortcut like `⌘K` is one key. */
export function keyParts(span: string): string[] | null {
  if (/^[⌘⇧⌥⌃]+[A-Z0-9]$/.test(span)) return [span];
  const parts = span.split('+');
  if (parts.some((p) => p.length === 0)) return null;
  const last = parts[parts.length - 1];
  const mods = parts.slice(0, -1);
  if (!mods.every((m) => MODIFIERS.has(m))) return null;
  if (isNamedKey(last) || MODIFIERS.has(last)) return parts;
  if (mods.length > 0 && (/^[A-Z0-9,./\\;=[\]-]$/.test(last) || ARROWS.has(last))) return parts;
  return null;
}

/** What a key reads on its keycap. Arrows read as arrows. */
export function keyGlyph(part: string): string {
  switch (part) {
    case 'ArrowUp':
    case 'Up':
      return '↑';
    case 'ArrowDown':
    case 'Down':
      return '↓';
    case 'ArrowLeft':
    case 'Left':
      return '←';
    case 'ArrowRight':
    case 'Right':
      return '→';
    default:
      return part;
  }
}

/** How to draw a backticked span. A key name is a key. A span that
 *  starts with a capital and holds only words, spaces, commas,
 *  apostrophes and `<placeholders>`, and may end in an ellipsis or a
 *  period, is a label you see in Vosh, like `Vosh is up to date.`.
 *  One that speaks to you the way the game does, like `You tell your
 *  group`, is game text, unless it names Vosh, which the game never
 *  does. Anything else is MUD text or a code. */
export function classifyInline(span: string): InlineKind {
  if (keyParts(span) !== null) return 'key';
  if (/^You /.test(span) && !/\bVosh\b/.test(span)) return 'code';
  if (/^[A-Z](?:[A-Za-z0-9 ,'’]|<[a-z]+>)*(?:…|\.)?$/.test(span) && /[a-z]/.test(span)) {
    return 'label';
  }
  return 'code';
}

/** One piece of a line of help text: plain text, or a backticked span
 *  and how to draw it. */
export type InlinePiece = { kind: 'text'; text: string } | { kind: InlineKind; text: string };

/** Split a line of help text at its backticks. */
export function inlinePieces(text: string): InlinePiece[] {
  const parts = text.split('`');
  const pieces: InlinePiece[] = [];
  parts.forEach((part, i) => {
    if (part.length === 0) return;
    if (i % 2 === 1) pieces.push({ kind: classifyInline(part), text: part });
    else pieces.push({ kind: 'text', text: part });
  });
  return pieces;
}
