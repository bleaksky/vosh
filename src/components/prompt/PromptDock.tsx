import { useEffect, useMemo, useState, type CSSProperties } from 'react';
import { subscribeBaseAnsi } from '../../lib/baseAnsi';
import { bandRuns, type BandEnv } from '../../lib/bandCells';
import type { PromptShowState } from '../../lib/promptShow';
import {
  BAND_LIFT,
  BAND_OUTSET_X,
  BAND_OUTSET_Y,
  bandRows,
  dockHeight,
  type CellSize,
} from '../../lib/promptBand';
import { shownColumns } from '../../lib/sgrCells';
import { usePinnedPrompt } from '../../lib/stores/pinnedPromptStore';
import { ansi16Of, xtermThemeFor } from '../../lib/terminalTheme';
import { getCurrentThemeId } from '../../lib/theme';
import { findTheme } from '../../lib/themes';

// Your prompt pinned above the command line (Where your prompt shows,
// Pinned). The session takes each prompt out of the text and sends it on
// the output's pin field, and this band shows the latest. It sits in the
// terminal area under the text, outside both renderers, so it looks the
// same over xterm and over the native grid.
//
// The dock keeps a fixed height, the most rows any prompt the capture
// reads can take, so the text above never moves when a fight adds a row.
// The band is drawn as the boards draw the edit band: --selrow, radius 4,
// 4 px past the text on each side and 2 px above and below its rows, its
// bottom 9.5 px above the input band. Each character sits on the
// terminal's own cell grid, so the columns line up with the text above.

/** The colors the terminal draws with now: the theme's, or the base
 *  palette while "Use the theme's colors for MUD text" is off. It follows
 *  a theme change (every apply writes data-theme on the root) and an
 *  edit to the base palette. */
function useBandEnv(
  themeTerminalColors: boolean,
  brightBold: boolean,
  renderer: BandEnv['renderer'],
): BandEnv {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const bump = () => setTick((n) => n + 1);
    const observer = new MutationObserver(bump);
    observer.observe(document.documentElement, {
      attributeFilter: ['data-theme', 'data-appearance'],
    });
    const unsubscribe = subscribeBaseAnsi(bump);
    return () => {
      observer.disconnect();
      unsubscribe();
    };
  }, []);
  return useMemo(() => {
    const resolved = xtermThemeFor(findTheme(getCurrentThemeId()), themeTerminalColors);
    return {
      palette: ansi16Of(resolved),
      fg: resolved.foreground ?? '#cccccc',
      bg: resolved.background ?? '#101218',
      renderer,
      brightBold,
    };
    // tick marks a theme or palette change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tick, themeTerminalColors, brightBold, renderer]);
}

interface PromptDockProps {
  state: PromptShowState;
  cell: CellSize;
  fontSize: number;
  themeTerminalColors: boolean;
  brightBold: boolean;
  renderer: BandEnv['renderer'];
}

export function PromptDock({
  state,
  cell,
  fontSize,
  themeTerminalColors,
  brightBold,
  renderer,
}: PromptDockProps) {
  const pin = usePinnedPrompt();
  const env = useBandEnv(themeTerminalColors, brightBold, renderer);
  return <PinnedBand state={state} pin={pin} cell={cell} fontSize={fontSize} env={env} />;
}

interface PinnedBandProps {
  state: PromptShowState;
  pin: string | null;
  cell: CellSize;
  fontSize: number;
  env: BandEnv;
}

/** The dock drawn from what it is handed. Exported for its test. */
export function PinnedBand({ state, pin, cell, fontSize, env }: PinnedBandProps) {
  const zone = Math.max(1, state.zone);
  const rows = useMemo(() => (pin ? bandRows(pin, zone) : []), [pin, zone]);
  const height = dockHeight(zone, cell.height);
  const limit = Math.max(1, cell.cols);
  const widths = rows.map((row) => Math.min(shownColumns(row), limit));
  const cols = widths.reduce((most, w) => Math.max(most, w), 0);
  const band: CSSProperties = {
    left: -BAND_OUTSET_X,
    bottom: BAND_LIFT,
    width: cols * cell.width + 2 * BAND_OUTSET_X,
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
      style={{ height }}
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
                    <Run key={i} run={run} cell={cell} text={text} />
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

function Run({
  run,
  cell,
  text,
}: {
  run: ReturnType<typeof bandRuns>[number];
  cell: CellSize;
  text: CSSProperties;
}) {
  const { look } = run;
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
      {look.decoration && (
        <span
          className="prompt-band-glyph"
          aria-hidden="true"
          style={{
            ...face,
            left,
            width,
            color: 'transparent',
            textDecorationLine: look.decoration,
            textDecorationStyle: look.decorationStyle,
            textDecorationColor: look.decorationColor ?? look.color,
          }}
        >
          {run.text}
        </span>
      )}
      {run.glyphs.map((glyph) =>
        glyph.ch.trim().length === 0 ? null : (
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
