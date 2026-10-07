import { useEffect, useMemo, type CSSProperties, type Ref } from 'react';
import { vitalsTextWatch } from '../ipc/vitals';
import { useBandEnv } from '../prompt/useBandEnv';
import { useSelected } from '../stores/session/sessionsStore';
import { useVitalsText } from '../stores/session/vitalsTextStore';
import { bandRuns, decorationLine, type BandEnv } from '../terminal/bandCells';
import type { Cell } from '../terminal/sgrCells';
import { nativeSurfaceEnabled } from '../terminal/terminalRenderer';
import { readPanelGameFace, usePanelFaceVersion } from './panelFace';
import { usePaneText } from './paneTextSize';
import { vitalsTextLines, type TextLine } from './vitalsTextFit';

// The Text style (Vitals Styles Q7 to Q9): your vitals text, which the
// session renders with your prompt's codes and pushes while this footer
// watches it, at the footer's width in terminal cells. It draws in the
// game face at your panel size, in the colors your pinned prompt takes
// (useBandEnv) and never lifted, on the footer's 18 and 12 px sides.
// Each row wraps at the spaces where the text at full values wraps, and
// a row with %{right} keeps its right part whole while its left part
// ends in an ellipsis (vitalsTextFit.ts). Under Hide vitals while your
// prompt is pinned only the rows that read your fight stay, and the
// footer goes when none are left.

/** The footer's sides, 18 px at the left and 12 at the right. */
const SIDES_PX = 30;

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
  const cell = useMemo(
    () => cellWidth(readPanelGameFace(), size),
    // faceVersion marks a face that loaded or changed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [size, faceVersion],
  );
  const cols = Math.max(1, Math.floor((width - SIDES_PX) / cell));
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
  const lines = useMemo(
    () => (text ? vitalsTextLines(text, cols, fightOnly) : []),
    [text, cols, fightOnly],
  );
  return <VitalsTextBlock lines={lines} env={env} fightOnly={fightOnly} sectionRef={hostRef} />;
}

/** The footer drawn from its lines, so a test draws every case. With no
 *  line it keeps no room and draws nothing, and stays only so the
 *  footer's width can be measured. */
export function VitalsTextBlock({
  lines,
  env,
  fightOnly = false,
  sectionRef,
}: {
  lines: readonly TextLine[];
  env: BandEnv;
  fightOnly?: boolean;
  sectionRef?: Ref<HTMLElement> | undefined;
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
            <Runs cells={line.left} env={env} />
          </span>
          {line.right && (
            <span className="panel-vitals-text-right">
              <Runs cells={line.right} env={env} />
            </span>
          )}
        </div>
      ))}
    </section>
  );
}

function Runs({ cells, env }: { cells: Cell[]; env: BandEnv }) {
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

let measureCanvas: HTMLCanvasElement | null = null;

/** One cell of `face` at `size` px, or 0.6 of the size where nothing
 *  can measure. */
function cellWidth(face: string, size: number): number {
  const guess = size * 0.6;
  if (typeof document === 'undefined') return guess;
  measureCanvas ??= document.createElement('canvas');
  const ctx = measureCanvas.getContext?.('2d');
  if (!ctx) return guess;
  ctx.font = `${size}px ${face}`;
  const width = ctx.measureText('0000000000').width / 10;
  return width > 0 ? width : guess;
}
