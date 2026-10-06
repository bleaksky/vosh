import { useMemo, type CSSProperties } from 'react';
import { bandRuns, type BandEnv, type UnderlineLine } from '../terminal/bandCells';
import type { PromptShowState } from '../ipc/prompt';
import {
  BAND_LIFT,
  BAND_OUTSET_X,
  BAND_OUTSET_Y,
  bandRows,
  dockHeight,
  dockRows,
  lentRows,
  type CellSize,
} from './pinnedDock';
import { shownColumns } from '../terminal/sgrCells';
import { usePinnedPrompt } from '../stores/session/pinnedPromptStore';
import { usePromptReach } from '../stores/session/promptReachStore';
import { useBandEnv } from './useBandEnv';
import { useBlinkShown } from '../lib/blink';
import type { Cell } from '../terminal/sgrCells';

// Your prompt pinned above the command line (Where your prompt shows,
// Pinned). The session takes each prompt out of the text and sends it on
// the output's pin field, and this band shows the latest. It sits in the
// terminal area under the text, outside both renderers, so it looks the
// same over xterm and over the native grid.
//
// The dock is exactly as tall as the band it draws now: its rows, the
// outsets and the lift, and the gap over them. Out of a fight the
// default design draws one row and in a fight two, so no empty row ever
// waits inside the band for a tank who is not there. Until 2026-10-01 the
// dock kept the most rows any prompt could take, and James saw that row
// standing empty out of a fight: "there should not be a blank line where
// the tank is supposed to be when not fighting." While prompts are off it
// keeps one row for the sentence that says so. Before your first prompt
// and after you disconnect it takes no room at all. Until 2026-10-06 it
// kept one empty row there so the first prompt never moved the text, and
// at the login menu, which the game ends with no prompt Vosh reads, that
// row and its gap stood empty under the menu like two blank lines.
//
// Above the band it keeps one blank line and 6 px of space, in a fight
// and out of one, as the game leaves a blank line before each prompt:
// "there's no space between prompt and last line now." Its place under
// the terminal is always that gap and one row. The rows past the first
// it borrows from the bottom of the terminal pane, reaching up over it
// with a negative top margin, and the pane gives them up through
// Terminal's lentRows: the pane keeps its size and the grid drops the
// rows from its top, so the newest line stays right above the band, the
// line at the top leaves for the scrollback and comes back when the band
// shrinks, and the page lays out nothing new. While the dock shows, the
// grid also keeps to the bottom of its pane (Terminal's anchorBottom), so
// the pixels a window leaves over under whole rows sit above the text and
// the newest line sits the same gap over the band in any window. MainWindow
// reads the same count from the same store, so the band and the terminal
// change in one commit, before the page paints. The game is told the rows
// the pane holds with a one row band, so a fight sends it no new size
// (src/terminal/terminalRows.ts).
//
// The band is drawn as the boards draw the edit band: --selrow, radius 4,
// 4 px past the text on each side and 2 px above and below its rows, its
// bottom 9.5 px above the input band. Each character sits on the
// terminal's own cell grid, so the columns line up with the text above.

interface PromptDockProps {
  state: PromptShowState;
  cell: CellSize;
  fontSize: number;
  themeTerminalColors: boolean;
  brightBold: boolean;
  renderer: BandEnv['renderer'];
  /** Blinking text is on, so your prompt blinks with the text. */
  blinkText: boolean;
}

export function PromptDock({
  state,
  cell,
  fontSize,
  themeTerminalColors,
  brightBold,
  renderer,
  blinkText,
}: PromptDockProps) {
  const pin = usePinnedPrompt();
  const reach = usePromptReach();
  const env = useBandEnv(themeTerminalColors, brightBold, renderer);
  return (
    <PinnedBand
      state={state}
      pin={pin}
      cell={cell}
      fontSize={fontSize}
      env={env}
      reach={reach}
      blinkText={blinkText}
    />
  );
}

/** A cell that blinks with something the hidden half takes away: a
 *  letter, an underline or a strike. */
function blinks(cell: Cell): boolean {
  const { attrs } = cell;
  return attrs.blink && (cell.ch.trim().length > 0 || attrs.underline !== 0 || attrs.strike);
}

interface PinnedBandProps {
  state: PromptShowState;
  pin: string | null;
  cell: CellSize;
  fontSize: number;
  env: BandEnv;
  /** How far past its widest row the band reaches for the prompt card's
   *  ↵ and caret, in px. */
  reach?: number;
  /** Blinking text is on. The band flips on the clock the terminal
   *  flips on, and only while a cell it draws blinks. */
  blinkText?: boolean;
}

/** The dock drawn from what it is handed. Exported for its test. */
export function PinnedBand({
  state,
  pin,
  cell,
  fontSize,
  env,
  reach: extra = 0,
  blinkText = false,
}: PinnedBandProps) {
  const zone = Math.max(1, state.zone);
  const rows = useMemo(() => (pin ? bandRows(pin, zone) : []), [pin, zone]);
  const blinking = useMemo(() => rows.some((row) => row.some(blinks)), [rows]);
  const blinkHidden = !useBlinkShown(blinkText && blinking);
  const shown = dockRows(pin, zone, state.promptsOff);
  if (shown === 0) return null;
  const height = dockHeight(shown, cell.height);
  // How far the dock reaches up over the terminal: the rows it borrows.
  const reach = lentRows(shown) * cell.height;
  const limit = Math.max(1, cell.cols);
  const widths = rows.map((row) => Math.min(shownColumns(row), limit));
  const cols = widths.reduce((most, w) => Math.max(most, w), 0);
  const band: CSSProperties = {
    left: -BAND_OUTSET_X,
    bottom: BAND_LIFT,
    width: cols * cell.width + 2 * BAND_OUTSET_X + extra,
    height: rows.length * cell.height + 2 * BAND_OUTSET_Y,
  };
  const text: CSSProperties = {
    fontSize: `${fontSize}px`,
    lineHeight: `${cell.height}px`,
    height: cell.height,
  };
  // The prompts off sentence sits on the bottom row slot.
  const noteTop = height - BAND_LIFT - BAND_OUTSET_Y - cell.height;
  return (
    <div
      className="prompt-dock"
      style={reach > 0 ? { height, marginTop: -reach } : { height }}
      data-rows={shown}
      role="status"
      aria-live="off"
      aria-label="Your prompt"
    >
      {state.promptsOff ? (
        <div
          className="prompt-dock-note"
          style={{ top: noteTop + (cell.height - 16) / 2 }}
          data-prompt-dock-note=""
        >
          You turned prompts off in the game. Type prompt in the game to turn them back on.
        </div>
      ) : (
        rows.length > 0 &&
        cols > 0 && (
          <div className="prompt-band" style={band} data-prompt-band="">
            {rows.map((row, r) => {
              const clipped = shownColumns(row) > limit;
              const runs = bandRuns(row, env, clipped ? limit - 1 : limit);
              const top = BAND_OUTSET_Y + r * cell.height;
              return (
                <div key={r} className="prompt-band-row" style={{ top }}>
                  {runs.map((run, i) => (
                    <Run key={i} run={run} cell={cell} text={text} blinkHidden={blinkHidden} />
                  ))}
                  {clipped && (
                    <span
                      className="prompt-band-glyph prompt-band-more"
                      style={{
                        ...text,
                        left: BAND_OUTSET_X + (limit - 1) * cell.width,
                        width: cell.width,
                      }}
                    >
                      …
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        )
      )}
    </div>
  );
}

/** One line across a run, drawn on the run's text in clear. */
function Line({
  run,
  face,
  left,
  width,
  line,
  style,
  color,
}: {
  run: ReturnType<typeof bandRuns>[number];
  face: CSSProperties;
  left: number;
  width: number;
  line: 'underline' | 'line-through';
  style: UnderlineLine;
  color: string;
}) {
  return (
    <span
      className="prompt-band-glyph"
      aria-hidden="true"
      style={{
        ...face,
        left,
        width,
        color: 'transparent',
        textDecorationLine: line,
        textDecorationStyle: style,
        textDecorationColor: color,
      }}
    >
      {run.text}
    </span>
  );
}

function Run({
  run,
  cell,
  text,
  blinkHidden,
}: {
  run: ReturnType<typeof bandRuns>[number];
  cell: CellSize;
  text: CSSProperties;
  /** Blinking text is in its hidden half. */
  blinkHidden: boolean;
}) {
  const { look } = run;
  // The hidden half of a blink takes the letters and both lines and
  // keeps the ground, as xterm and the native grid draw it.
  const hidden = blinkHidden && look.blink;
  const left = BAND_OUTSET_X + run.col * cell.width;
  const width = run.cols * cell.width;
  const face: CSSProperties = {
    ...text,
    color: look.color,
    fontWeight: look.bold ? 700 : 400,
    fontStyle: look.italic ? 'italic' : 'normal',
  };
  return (
    <>
      {look.background && (
        <span
          className="prompt-band-ground"
          style={{ left, width, height: cell.height, background: look.background }}
        />
      )}
      {/* The underline and the strike each draw on a run of their own,
          since CSS gives the lines of one box one style and one color,
          and the strike is always straight in the text color. */}
      {look.underline && !hidden && (
        <Line
          run={run}
          face={face}
          left={left}
          width={width}
          line="underline"
          style={look.underline.style}
          color={look.underline.color ?? look.color}
        />
      )}
      {look.strike && !hidden && (
        <Line
          run={run}
          face={face}
          left={left}
          width={width}
          line="line-through"
          style="solid"
          color={look.color}
        />
      )}
      {run.glyphs.map((glyph) =>
        hidden || glyph.ch.trim().length === 0 ? null : (
          <span
            key={glyph.col}
            className="prompt-band-glyph"
            style={{
              ...face,
              left: BAND_OUTSET_X + glyph.col * cell.width,
              width: glyph.cols * cell.width,
            }}
          >
            {glyph.ch}
          </span>
        ),
      )}
    </>
  );
}
