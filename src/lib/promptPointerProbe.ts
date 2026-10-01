// A dev console probe for the prompt card's pointer mapping, the phase 4
// gate of the prompt build spec. A dev run (npm run tauri dev) installs it
// as window.__voshPromptPointer, and a production build never loads it.
// Call __voshPromptPointer(true) in the Web Inspector, then click parts of
// your prompt in the terminal or on the pinned band. The console names the
// piece under each click and the characters it covers. Call it with false
// to stop. Clicks still do what they always do.

import type { CellSize } from './promptBand';
import {
  bandPlain,
  dockCellAt,
  dockPieceAt,
  pieceAtCell,
  pieceText,
  type OpenPrompt,
  type PieceSpan,
  type RegionOnScreen,
  type ScreenCell,
} from './promptPointer';

/** What the probe reads, so a test can hand it fakes. */
export interface ProbeDeps {
  renderer: () => 'xterm' | 'native';
  /** The live terminal pane, or null before it mounts. */
  terminal: () => {
    promptRegion: () => Promise<RegionOnScreen | null>;
    cellAt: (clientX: number, clientY: number) => ScreenCell | null;
  } | null;
  /** The open row as prompt_state_get reports it. */
  openRow: () => Promise<OpenPrompt | null>;
  /** The dock's box in client px with its zone and cell, or null while
   *  your prompt does not show pinned. */
  dock: () => {
    left: number;
    top: number;
    right: number;
    bottom: number;
    zone: number;
    cell: CellSize;
  } | null;
  /** What the band shows, with its pieces. */
  band: () => { text: string; spans: readonly PieceSpan[] } | null;
}

/** What the probe found under a point. */
export type ProbeHit =
  | {
      where: 'text';
      renderer: 'xterm' | 'native';
      cell: ScreenCell;
      /** The open region the renderer holds, or null. */
      gen: number | null;
      piece: number | null;
      text: string | null;
    }
  | { where: 'band'; cell: ScreenCell | null; piece: number | null; text: string | null };

/** The piece of your prompt under a point in client px: on the pinned
 *  band when the point is on the dock, else on the open row in the
 *  terminal. Null off both. */
export async function probePoint(
  clientX: number,
  clientY: number,
  deps: ProbeDeps,
): Promise<ProbeHit | null> {
  const dock = deps.dock();
  if (
    dock &&
    clientX >= dock.left &&
    clientX < dock.right &&
    clientY >= dock.top &&
    clientY < dock.bottom
  ) {
    const band = deps.band();
    const x = clientX - dock.left;
    const y = clientY - dock.top;
    const piece = dockPieceAt(band, dock.zone, dock.cell, x, y);
    return {
      where: 'band',
      cell: band ? dockCellAt(band.text, dock.zone, dock.cell, x, y) : null,
      piece,
      text: band && piece !== null ? pieceText(bandPlain(band.text), band.spans, piece) : null,
    };
  }
  const terminal = deps.terminal();
  const cell = terminal?.cellAt(clientX, clientY) ?? null;
  if (!terminal || !cell) return null;
  const [region, open] = await Promise.all([terminal.promptRegion(), deps.openRow()]);
  const piece = pieceAtCell(open, region, cell);
  return {
    where: 'text',
    renderer: deps.renderer(),
    cell,
    gen: region?.gen ?? null,
    piece,
    text: open && piece !== null ? pieceText(open.plain, open.spans, piece) : null,
  };
}

/** Install window.__voshPromptPointer. Returns what removes it. */
export function installPromptPointerProbe(deps: ProbeDeps): () => void {
  let listening: ((event: PointerEvent) => void) | null = null;
  const stop = () => {
    if (listening) window.removeEventListener('pointerdown', listening, true);
    listening = null;
  };
  const probe = (on = true): string => {
    stop();
    if (!on) return 'The prompt pointer probe is off.';
    listening = (event: PointerEvent) => {
      void probePoint(event.clientX, event.clientY, deps).then((hit) => {
        console.log('[vosh prompt pointer]', hit);
      });
    };
    window.addEventListener('pointerdown', listening, true);
    return 'The prompt pointer probe is on. Click parts of your prompt.';
  };
  const host = window as { __voshPromptPointer?: (on?: boolean) => string };
  host.__voshPromptPointer = probe;
  return () => {
    stop();
    if (host.__voshPromptPointer === probe) delete host.__voshPromptPointer;
  };
}
