import type { EditorLine } from './editorLine';

// The tick at the right edge of the width, while the game's editor holds
// a text Vosh names. What runs past it takes a wash (washPast), and the
// pill at the line's start counts the lines.

export function EditorMarks({
  field,
  cell,
  editor,
}: {
  field: HTMLElement | null;
  cell: number;
  editor: EditorLine;
}) {
  if (cell === 0) return null;
  return (
    <span
      className="wr-tick"
      aria-hidden="true"
      style={{ left: (field?.offsetLeft ?? 0) + editor.width * cell }}
    />
  );
}
