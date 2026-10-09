import { useEffect, useMemo, useState, type CSSProperties, type Ref } from 'react';
import { promptRenderMany, type PromptRendered } from '../ipc/promptDesign';
import { vitalsTextWatch, type VitalsText as RenderedText } from '../ipc/vitals';
import { useBandEnv } from '../prompt/useBandEnv';
import { useSelected } from '../stores/session/sessionsStore';
import { useVitalsText } from '../stores/session/vitalsTextStore';
import { useVitalsCardMarks, type VitalsCardMarks } from '../stores/session/vitalsCardStore';
import { bandRuns, decorationLine, type BandEnv } from '../terminal/bandCells';
import type { Cell } from '../terminal/sgrCells';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { readPanelGameFace, usePanelFaceVersion } from './panelFace';
import { usePaneText } from './paneTextSize';
import { textCols, vitalsTextLines, type PieceCell, type TextLine } from './vitalsTextFit';

// The Text style: your vitals text, which the
// session renders with your prompt's codes and pushes while this footer
// watches it, at the footer's width in terminal cells. It draws in the
// game face at your panel size, in the colors your pinned prompt takes
// (useBandEnv) and never lifted, on the footer's 18 and 12 px sides.
// Each row wraps at the spaces where the text at full values wraps, and
// a row with %{right} keeps its right part whole while its left part
// ends in an ellipsis (vitalsTextFit.ts). Under Hide vitals while your
// prompt is pinned only the rows that read your fight stay, and the
// footer goes when none are left.
//
// While the vitals text card is open the footer rings the part you
// picked, a click on a part turns the card to it, and a preview the
// card picks draws here, wrapped on its own values.

/** The terminal settings your text draws its colors with. */
export interface TextColors {
  themeTerminalColors: boolean;
  brightBold: boolean;
}

/** `width` is the footer's in px, measured on `hostRef`, the section
 *  this draws. */
export function VitalsText({
  width,
  hostRef,
  colors,
  fightOnly,
}: {
  width: number;
  hostRef?: Ref<HTMLElement>;
  colors: TextColors;
  fightOnly: boolean;
}) {
  const session = useSelected();
  const text = useVitalsText();
  const { size } = usePaneText();
  const faceVersion = usePanelFaceVersion();
  const cols = useMemo(
    () => textCols(width, readPanelGameFace(), size),
    // faceVersion marks a face that loaded or changed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [width, size, faceVersion],
  );
  // Watch while this footer draws the Text style. A new width or
  // session watches again, and a new style or place, which unmounts
  // this, stops it.
  useEffect(() => {
    void vitalsTextWatch(cols, session).catch(() => undefined);
    return () => {
      void vitalsTextWatch(null, session).catch(() => undefined);
    };
  }, [cols, session]);
  const env = useBandEnv(
    colors.themeTerminalColors,
    colors.brightBold,
    nativeSurfaceEnabled() ? 'native' : 'xterm',
  );
  const marks = useVitalsCardMarks();
  const previewed = usePreviewed(text, marks, cols, session);
  const shown = previewed ?? text;
  const lines = useMemo(
    () => (shown ? vitalsTextLines(shown, cols, fightOnly, marks !== null) : []),
    [shown, cols, fightOnly, marks],
  );
  return (
    <VitalsTextBlock
      lines={lines}
      env={env}
      fightOnly={fightOnly}
      sectionRef={hostRef}
      marks={marks}
    />
  );
}

/** `text` as the card's preview draws it, or null at Now and while the
 *  card is closed. A preview row stays under Hide vitals while your
 *  prompt is pinned, since the preview is what you asked to see. */
function usePreviewed(
  text: RenderedText | null,
  marks: VitalsCardMarks | null,
  cols: number,
  session: number,
): RenderedText | null {
  const template = marks?.template ?? null;
  const preview = marks?.preview ?? 'now';
  const [rendered, setRendered] = useState<PromptRendered | null>(null);
  useEffect(() => {
    if (template === null || preview === 'now') return;
    let open = true;
    void promptRenderMany([{ template, values: 'live', preview, cols }], session)
      .then(([drawn]) => open && setRendered(drawn ?? null))
      .catch(() => open && setRendered(null));
    return () => {
      open = false;
      setRendered(null);
    };
  }, [template, preview, cols, session]);
  return useMemo(
    () =>
      text && rendered
        ? {
            ...text,
            live: rendered,
            full: rendered,
            fight: Array.from({ length: rendered.rows }, () => true),
          }
        : null,
    [text, rendered],
  );
}

/** The footer drawn from its lines, so a test draws every case. With no
 *  line it keeps no room and draws nothing, and stays only so the
 *  footer's width can be measured. */
export function VitalsTextBlock({
  lines,
  env,
  fightOnly = false,
  sectionRef,
  marks = null,
}: {
  lines: readonly TextLine[];
  env: BandEnv;
  fightOnly?: boolean;
  sectionRef?: Ref<HTMLElement> | undefined;
  /** The vitals text card's marks while it is open. */
  marks?: VitalsCardMarks | null;
}) {
  if (lines.length === 0) {
    return <section ref={sectionRef} className="panel-vitals-text is-empty" aria-hidden="true" />;
  }
  return (
    <section
      ref={sectionRef}
      className="panel-vitals-text"
      style={{ color: env.fg }}
      aria-label={fightOnly ? 'Opponent' : 'Vitals'}
    >
      {lines.map((line, i) => (
        <div key={i} className="panel-vitals-text-row">
          <span className="panel-vitals-text-left">
            <Side cells={line.left} env={env} marks={marks} />
          </span>
          {line.right && (
            <span className="panel-vitals-text-right">
              <Side cells={line.right} env={env} marks={marks} />
            </span>
          )}
        </div>
      ))}
    </section>
  );
}

/** One side of a line: its runs, or while the card is open each part
 *  of your text on its own, clickable, the one you picked ringed. */
function Side({
  cells,
  env,
  marks,
}: {
  cells: PieceCell[];
  env: BandEnv;
  marks: VitalsCardMarks | null;
}) {
  if (!marks) return <TextRuns cells={cells} env={env} />;
  const parts: { piece: number | undefined; cells: PieceCell[] }[] = [];
  for (const cell of cells) {
    const last = parts[parts.length - 1];
    if (last && last.piece === cell.piece) last.cells.push(cell);
    else parts.push({ piece: cell.piece, cells: [cell] });
  }
  return parts.map((part, i) => {
    const { piece } = part;
    if (piece === undefined) return <TextRuns key={i} cells={part.cells} env={env} />;
    return (
      <span
        key={i}
        className={`panel-vitals-text-part${marks.picked === piece ? ' is-picked' : ''}`}
        // The card keeps focus, so the keys that work on a part reach it.
        onMouseDown={(e) => e.preventDefault()}
        onClick={() => marks.pick(piece)}
      >
        <TextRuns cells={part.cells} env={env} />
      </span>
    );
  });
}

/** `cells` as runs of one look each, in the colors `env` draws. */
export function TextRuns({ cells, env }: { cells: Cell[]; env: BandEnv }) {
  return bandRuns(cells, env, Number.POSITIVE_INFINITY).map((run) => {
    const { look } = run;
    const style: CSSProperties = {
      color: look.color,
      background: look.background ?? undefined,
      fontWeight: look.bold ? 700 : undefined,
      fontStyle: look.italic ? 'italic' : undefined,
      textDecorationLine: decorationLine(look),
      textDecorationStyle: look.underline?.style,
      textDecorationColor: look.underline?.color ?? undefined,
    };
    return (
      <span key={run.col} style={style}>
        {run.text}
      </span>
    );
  });
}
