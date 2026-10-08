import { useCallback, useEffect, useRef, useState, type CSSProperties } from 'react';
import { bandCut, bandRowsTop, type CellSize } from './pinnedDock';
import {
  dockMapper,
  endPlace,
  layoutMarks,
  rawLayout,
  textMapper,
  type Box,
  type MarkLayout,
  type Pointing,
  type RawMark,
} from './promptPieces';
import { bandPlain, dockCellAt, dockPieceAt, layoutPrompt, pieceAtCell } from './promptPointer';
import { findOnScreen, type ScreenAsk } from './promptScreen';
import type { PromptOpenRow, PromptShowState } from '../ipc/prompt';
import { type PromptPiece } from '../ipc/promptDesign';
import { onOutput } from '../ipc/terminal';
import { getPinnedBand } from '../stores/session/pinnedPromptStore';
import { getSelected } from '../stores/session/sessionsStore';
import { setPromptReach } from '../stores/session/promptReachStore';
import { shownColumns } from '../terminal/sgrCells';
import type { PromptCardHost } from './PromptCard';

// The prompt card's marks on your prompt: the part you picked with the
// accent tint and a 1 px accent ring, the caret where what you add
// goes, a ↵ after each row a line break ends, and the warn ring on a
// part Vosh cannot fill. While the card reads your codes the tint sits
// on each value Vosh reads in the game's own line. They are DOM over
// the terminal, tints and 1 px rings that leave the glyphs readable.
// The edit band under them is the renderer's own band pass, never a
// fill here. They draw only while the terminal shows its newest rows.
//
// A click on a part of your prompt picks it, a click past its last cell
// puts the caret there, and a click on a ↵ picks the line break. Clicks
// anywhere else go on to the terminal as usual.

interface PromptMarksProps {
  host: PromptCardHost;
  show: PromptShowState | null;
  cell: CellSize | null;
  /** The open row as the card last read it. */
  openRow: PromptOpenRow | null;
  /** Your design's parts and what the card points at, while it draws it. */
  design: {
    pieces: readonly PromptPiece[];
    pointing: Pointing;
    warn: ReadonlySet<number>;
  } | null;
  /** The marks on the game's own lines while the card reads your codes. */
  raw: readonly RawMark[] | null;
  /** The game's own line to find on screen, where no open row shows it:
   *  while the profile reads no prompt, and while you point at the line
   *  on another game. */
  screen: ScreenAsk | null;
  refresh: number;
  onPoint: (pointing: Pointing) => void;
  /** The card, whose own clicks are its own. */
  card: () => HTMLElement | null;
}

interface Measured {
  layout: MarkLayout;
  /** What a click at a point in client px points at, or null off your
   *  prompt. */
  hit: ((x: number, y: number) => Pointing | null) | null;
  /** The client x one past the last glyph of your prompt's widest row,
   *  where its band reaches from, or null with no band to widen. */
  textRight: number | null;
}

const NONE: MarkLayout = { picked: [], values: [], warn: [], returns: [], caret: null };

function boxStyle(box: Box): CSSProperties {
  return { left: box.left, top: box.top, width: box.width, height: box.height };
}

export function PromptMarks({
  host,
  show,
  cell,
  openRow,
  design,
  raw,
  screen,
  refresh,
  onPoint,
  card,
}: PromptMarksProps) {
  const [layout, setLayout] = useState<MarkLayout>(NONE);
  const measured = useRef<Measured | null>(null);
  const pinned = show?.show === 'pinned' && show.capture;

  const measure = useCallback(async (): Promise<Measured | null> => {
    if (pinned) {
      const dock = host.dock();
      const band = getPinnedBand();
      if (!dock || !band || !cell) return null;
      const r = dock.getBoundingClientRect();
      const rows = Number(dock.getAttribute('data-rows')) || 1;
      const cut = bandCut(band.text, rows);
      const shown = cut.rows.length;
      const plain = bandPlain(band.text).split('\n');
      const mapper = dockMapper({
        left: r.left,
        rowsTop: r.top + bandRowsTop(rows, shown, cell.height),
        first: cut.first,
        shown,
        cellW: cell.width,
        cellH: cell.height,
        rows: plain,
      });
      if (!design) {
        if (!raw || !openRow) return null;
        // The game's own lines are the band's last rows.
        const from = plain.length - openRow.raw_lines.length;
        return {
          layout: rawLayout(
            raw.map((m) => ({ ...m, row: m.row + from })),
            mapper,
          ),
          hit: null,
          textRight: null,
        };
      }
      const lineBreaks = new Set(design.pieces.filter((p) => p.kind === 'nl').map((p) => p.piece));
      const layout = layoutMarks({
        spans: band.spans,
        lineBreaks,
        mapper,
        grid: { cellW: cell.width, cellH: cell.height },
        pointing: design.pointing,
        warn: design.warn,
      });
      const hit = (x: number, y: number): Pointing | null => {
        if (x < r.left || x >= r.right || y < r.top || y >= r.bottom) return null;
        const piece = dockPieceAt(band, rows, cell, x - r.left, y - r.top);
        if (piece !== null) return { picked: piece, caret: null };
        const at = dockCellAt(band.text, rows, cell, x - r.left, y - r.top);
        return at ? { picked: null, caret: endPlace(design.pieces) } : null;
      };
      // The band reaches to the last glyph of its widest row, as the
      // dock counts it.
      const widest = Math.max(0, ...cut.rows.map((row) => Math.min(shownColumns(row), cell.cols)));
      return { layout, hit, textRight: r.left + widest * cell.width };
    }
    const term = host.terminal();
    const grid = term?.grid() ?? null;
    if (!term || !grid) return null;
    // No open row shows the game's line, so the marks find it on screen:
    // the last text there while Vosh reads your codes, every row of its
    // shape while you point at it.
    if (!design && screen && (screen.mode === 'shape' || !openRow)) {
      const shown = await term.screenRows().catch(() => null);
      if (!shown?.atBottom) return null;
      const layouts = findOnScreen(shown.rows, screen, shown.cols).map((found) =>
        rawLayout(
          screen.marks(found.lines),
          textMapper(
            found.lines.join('\n'),
            { gen: 0, row: found.row, col: 0, cols: shown.cols, atBottom: true },
            grid,
          ),
        ),
      );
      return {
        layout: {
          ...NONE,
          values: layouts.flatMap((l) => l.values),
          warn: layouts.flatMap((l) => l.warn),
        },
        hit: null,
        textRight: null,
      };
    }
    if (!openRow) return null;
    const region = await term.promptRegion().catch(() => null);
    if (!region || region.gen !== openRow.gen || !region.atBottom) return null;
    if (!design) {
      if (!raw) return null;
      return {
        layout: rawLayout(raw, textMapper(openRow.raw_lines.join('\n'), region, grid)),
        hit: null,
        textRight: null,
      };
    }
    if (openRow.spans.length === 0) return null;
    const lineBreaks = new Set(design.pieces.filter((p) => p.kind === 'nl').map((p) => p.piece));
    const layout = layoutMarks({
      spans: openRow.spans,
      lineBreaks,
      mapper: textMapper(openRow.plain, region, grid),
      grid,
      pointing: design.pointing,
      warn: design.warn,
    });
    const placed = layoutPrompt(openRow.plain, region.col, region.cols);
    const lastRow = Math.max(0, ...placed.flat().map((p) => p.row));
    // One past the last glyph of the widest screen row, as a band counts.
    const rowTexts = openRow.plain.split('\n').map((row) => Array.from(row));
    let rightmost = 0;
    placed.forEach((row, r) =>
      row.forEach((p, i) => {
        if (p.width > 0 && (rowTexts[r]?.[i] ?? ' ').trim() !== '') {
          rightmost = Math.max(rightmost, p.col + p.width);
        }
      }),
    );
    const hit = (x: number, y: number): Pointing | null => {
      const at = term.cellAt(x, y);
      if (!at) return null;
      const piece = pieceAtCell(openRow, region, at);
      if (piece !== null) return { picked: piece, caret: null };
      // On a row of your prompt past its last cell, the caret goes to
      // the end.
      if (at.row >= region.row && at.row <= region.row + lastRow) {
        return { picked: null, caret: endPlace(design.pieces) };
      }
      return null;
    };
    return { layout, hit, textRight: grid.left + rightmost * grid.cellW };
  }, [pinned, host, cell, openRow, design, raw, screen]);

  const run = useCallback(() => {
    let alive = true;
    void measure().then((next) => {
      if (!alive) return;
      measured.current = next;
      const shown = next?.layout ?? NONE;
      setLayout(shown);
      // The band reaches past its widest row for the ↵ and caret.
      const rights = [
        ...shown.returns.map((r) => r.box.left + r.box.width),
        ...(shown.caret ? [shown.caret.left + shown.caret.width] : []),
      ];
      const textRight = next?.textRight ?? null;
      setPromptReach(textRight === null ? 0 : Math.max(0, ...rights.map((x) => x - textRight)));
    });
    return () => {
      alive = false;
    };
  }, [measure]);

  useEffect(() => run(), [run, refresh]);

  // The marks follow the terminal as it resizes or scrolls.
  useEffect(() => {
    const area = host.area();
    let frame = 0;
    const later = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => run());
    };
    const observer = area ? new ResizeObserver(later) : null;
    if (area) observer?.observe(area);
    window.addEventListener('resize', later);
    area?.addEventListener('wheel', later, { passive: true });
    return () => {
      cancelAnimationFrame(frame);
      observer?.disconnect();
      window.removeEventListener('resize', later);
      area?.removeEventListener('wheel', later);
    };
  }, [host, run]);

  // Found on screen, the line moves as the game's text arrives, so the
  // marks look again once the terminal has written it.
  const finding = screen !== null;
  useEffect(() => {
    if (!finding) return;
    let timer = 0;
    let stop: (() => void) | null = null;
    let alive = true;
    // Only the terminal of the session in front moves the line.
    void onOutput(
      (session) => session === getSelected(),
      () => {
        window.clearTimeout(timer);
        timer = window.setTimeout(() => run(), 60);
      },
    ).then((fn) => (alive ? (stop = fn) : fn()));
    return () => {
      alive = false;
      window.clearTimeout(timer);
      stop?.();
    };
  }, [finding, run]);

  useEffect(() => () => setPromptReach(0), []);

  // A click on your prompt picks a part or places the caret. The card
  // takes it whole, so the terminal neither selects nor takes focus.
  const pointRef = useRef(onPoint);
  pointRef.current = onPoint;
  const cardRef = useRef(card);
  cardRef.current = card;
  useEffect(() => {
    const card = () => cardRef.current();
    const swallow = (e: Event) => {
      e.stopPropagation();
      e.preventDefault();
    };
    const onDown = (e: PointerEvent) => {
      if (e.button !== 0 || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return;
      const target = e.target instanceof Element ? e.target : null;
      // The card, its menus and dialogs, and a line break mark handle
      // their own presses.
      if (
        target &&
        (card()?.contains(target) ||
          target.closest('[data-menu-surface], [role="dialog"], .pc-mark-return'))
      ) {
        return;
      }
      const hit = measured.current?.hit?.(e.clientX, e.clientY) ?? null;
      if (!hit) return;
      swallow(e);
      window.addEventListener('mouseup', swallow, { capture: true, once: true });
      window.addEventListener('click', swallow, { capture: true, once: true });
      pointRef.current(hit);
      card()?.focus({ preventScroll: true });
    };
    // xterm starts a selection on mousedown, which a page may still send
    // after a pointerdown the card took.
    const onMouseDown = (e: MouseEvent) => {
      const target = e.target instanceof Element ? e.target : null;
      if (target?.closest('.pc-mark-return')) return;
      if (e.button === 0 && measured.current?.hit?.(e.clientX, e.clientY)) swallow(e);
    };
    window.addEventListener('pointerdown', onDown, true);
    window.addEventListener('mousedown', onMouseDown, true);
    return () => {
      window.removeEventListener('pointerdown', onDown, true);
      window.removeEventListener('mousedown', onMouseDown, true);
    };
  }, []);

  return (
    <>
      <div className="pc-marks" aria-hidden="true">
        {layout.warn.map((b, i) => (
          <span key={`w${i}`} className="pc-mark-warn" style={boxStyle(b)} />
        ))}
        {layout.values.map((b, i) => (
          <span key={`v${i}`} className="pc-mark-value" style={boxStyle(b)} />
        ))}
        {layout.picked.map((b, i) => (
          <span key={`p${i}`} className="pc-mark-token" style={boxStyle(b)} />
        ))}
        {layout.caret && <span className="pc-mark-caret" style={boxStyle(layout.caret)} />}
      </div>
      {layout.returns.map((r) => (
        <button
          key={r.piece}
          type="button"
          tabIndex={-1}
          className="pc-mark-return"
          aria-label="Line break"
          aria-pressed={r.picked}
          style={boxStyle(r.box)}
          onPointerDown={(e) => {
            e.preventDefault();
            e.stopPropagation();
            onPoint({ picked: r.piece, caret: null });
            card()?.focus({ preventScroll: true });
          }}
        >
          ↵
        </button>
      ))}
    </>
  );
}
