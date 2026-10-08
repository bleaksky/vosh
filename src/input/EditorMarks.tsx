import { editorCount, type EditorLine } from './editorLine';

// The tick at the right edge of the width and the count at the command
// line's right, while the game's editor holds a text Vosh names
// (Description Editor board 3). Past the width the count turns danger or
// warn, and what runs past the tick takes the same wash.

export function EditorMarks({
  field,
  cell,
  line,
  editor,
}: {
  field: HTMLElement | null;
  cell: number;
  line: string;
  editor: EditorLine;
}) {
  const count = editorCount(line, editor);
  return (
    <>
      {cell > 0 && (
        <span
          className="wr-tick"
          aria-hidden="true"
          style={{ left: (field?.offsetLeft ?? 0) + editor.width * cell }}
        />
      )}
      <span className={`wr-cl-count${count.tone ? ` is-${count.tone}` : ''}`} aria-live="polite">
        {count.text}
      </span>
    </>
  );
}
