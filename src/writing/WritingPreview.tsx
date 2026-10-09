import { codeSlot } from './gameCodes';

// The text as a looker or a reader gets it, from Preview in the ⋯ menu:
// no gutter, guide or marks, the colors the codes draw, and for a note
// the two lines the game's show prints over it. A code inside a line
// colors the rest only from trust 55, as the game keeps it
// (comm.c:1499).

interface Piece {
  text: string;
  color: string | null;
  bold: boolean;
}

/** `line` in the pieces its codes color. */
function pieces(line: string, palette: readonly string[], immortal: boolean): Piece[] {
  const out: Piece[] = [];
  let color: string | null = null;
  let bold = false;
  let text = '';
  const flush = () => {
    if (text) out.push({ text, color, bold });
    text = '';
  };
  for (let i = 0; i < line.length; i += 1) {
    if (line[i] === '`' && i + 1 < line.length && (i === 0 || immortal)) {
      flush();
      const slot = codeSlot(line[i + 1]);
      color = slot ? (palette[slot.color] ?? null) : null;
      bold = slot?.bold ?? false;
      i += 1;
      continue;
    }
    if (line[i] === '`' && i + 1 < line.length) {
      i += 1;
      continue;
    }
    text += line[i];
  }
  flush();
  return out;
}

export function WritingPreview({
  head,
  lines,
  palette,
  immortal,
  width,
}: {
  /** The game's own lines over a note's text. */
  head: string[];
  lines: readonly string[];
  palette: readonly string[];
  immortal: boolean;
  width: number;
}) {
  return (
    <div className="wr-preview" style={{ width }}>
      {head.map((line) => (
        <div key={line} className="wr-preview-line">
          {line}
        </div>
      ))}
      {lines.map((line, k) => (
        <div key={k} className="wr-preview-line">
          {pieces(line, palette, immortal).map((p, n) => (
            <span
              key={n}
              style={p.color ? { color: p.color, fontWeight: p.bold ? 700 : undefined } : undefined}
            >
              {p.text}
            </span>
          ))}
          {line.length === 0 && ' '}
        </div>
      ))}
    </div>
  );
}
