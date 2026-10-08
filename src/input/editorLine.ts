import { useLayoutEffect, useState, type CSSProperties } from 'react';
import type { WritingState } from '../ipc/writing';
import { cardTakes, KINDS } from '../writing/kinds';
import { columns, startsAsCommand } from '../writing/text';

// The command line while the game's own line editor holds a text Vosh
// names, after you typed description edit or note edit and kept typing
// there, or opened one the card does not take yet with scribe text,
// vote edit, write edit or petedit desc. Each line goes raw, a paste
// goes on the game's > as the card's lines do, and the line shows a
// tick at the right edge of column 75 with its count.

/** What the command line shows of the editor, or null while it holds no
 *  text Vosh names, or the card drives it. */
export interface EditorLine {
  kind: NonNullable<WritingState['editor']>;
  width: number;
  /** A help sets the width, so a line past it reads in danger. */
  helpWidth: boolean;
}

export function editorLineOf(writing: WritingState): EditorLine | null {
  if (writing.editor === null || writing.job !== null) return null;
  const kind = writing.editor;
  return { kind, width: 75, helpWidth: cardTakes(kind) && KINDS[kind].helpWidth };
}

/** The count at the line's right and its tone: past the width it reads
 *  in danger where a help sets the width and in warn where Vosh does,
 *  and a line the editor would not take as text, one that starts with
 *  a dot, @ or ! or is only ~, reads in warn (olc.c:3613, 3746,
 *  comm.c:1560). */
export function editorCount(
  line: string,
  editor: EditorLine,
): { text: string; tone: '' | 'warn' | 'bad' } {
  const cols = columns(line);
  const text = `${cols} / ${editor.width}`;
  if (cols > editor.width) return { text, tone: editor.helpWidth ? 'bad' : 'warn' };
  if (startsAsCommand(line) || line.trim() === '~') return { text, tone: 'warn' };
  return { text, tone: '' };
}

/** What the held sends' chip says. */
export function heldLine(held: number): string {
  return held === 1 ? '1 send waits' : `${held} sends wait`;
}

/** The width of one column of the command line's face. */
export function useFieldCell(field: HTMLElement | null): number {
  const [cell, setCell] = useState(0);
  useLayoutEffect(() => {
    if (!field) return;
    const ctx = document.createElement('canvas').getContext('2d');
    if (!ctx) return;
    ctx.font = getComputedStyle(field).font;
    setCell(ctx.measureText('0000000000').width / 10);
  }, [field]);
  return cell;
}

/** The wash over what runs past the width, on the field's own ground. */
export function washPast(
  line: string,
  editor: EditorLine,
  cell: number,
): CSSProperties | undefined {
  const cols = columns(line);
  if (cell === 0 || cols <= editor.width) return undefined;
  const from = editor.width * cell;
  const to = cols * cell;
  const tone = editor.helpWidth ? 'var(--danger)' : 'var(--warn)';
  const wash = `color-mix(in srgb, ${tone} 18%, transparent)`;
  return {
    backgroundImage: `linear-gradient(to right, transparent ${from}px, ${wash} ${from}px, ${wash} ${to}px, transparent ${to}px)`,
    backgroundRepeat: 'no-repeat',
  };
}
