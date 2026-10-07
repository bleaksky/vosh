import { useLayoutEffect, useRef, useState } from 'react';
import { DrawnOpponent, DrawnVitals, MarkRows, type DrawnVitalsProps } from './VitalsDrawn';
import { blockCells, blockRun, FULL_BLOCK, type RowMarkFit } from './vitalsDrawnFit';
import { hitFill, type HitView } from './vitalsHit';

// Blocks (More Vitals Styles, board 1): the bar in the game face at
// your panel size, so it lines up with your prompt, as btop draws its
// meters. Each bar counts the cells that fit between the label and the
// widest value and writes a full block for each whole cell and an
// eighth for the last, over the same cells at a fifth of the tone, so
// the last cell never leaves a gap. The label and value share the face.
// Your opponent's bar runs the footer. With Show each hit on, the cells
// a hit took stay pale between, and go as the drain starts, since a
// cell cannot drain. On a narrow panel each bar drops under its label
// and value (vitalsDrawnFit.ts).

export function VitalsBlocks({
  rows,
  waiting,
  combat,
  place,
  inks,
  hits,
  fit,
}: DrawnVitalsProps & { fit: RowMarkFit }) {
  const foe = combat && (
    <DrawnOpponent
      combat={combat}
      rows={rows}
      waiting={waiting}
      place={place}
      under={(health) => <BlockBar pct={health.pct} hit={hits.foe} />}
    />
  );
  return (
    <DrawnVitals kind="blocks" waiting={waiting} place={place} foe={foe}>
      <MarkRows
        kind="blocks"
        under={fit === 'under'}
        rows={rows}
        inks={inks}
        mark={(row) => <BlockBar pct={row.pct} hit={hits[row.key]} />}
      />
    </DrawnVitals>
  );
}

function BlockBar({ pct, hit }: { pct: number | null; hit: HitView | undefined }) {
  const ref = useRef<HTMLSpanElement | null>(null);
  const cells = useCells(ref);
  const { fill, ghost, draining } = hitFill(pct, hit);
  return (
    <span ref={ref} className="vitals-blocks">
      <span className="vitals-blocks-unlit">{FULL_BLOCK.repeat(cells)}</span>
      {ghost !== null && !draining && (
        <span className="vitals-blocks-gone">{blockRun(ghost, cells)}</span>
      )}
      {fill !== null && <span className="vitals-blocks-lit">{blockRun(fill, cells)}</span>}
    </span>
  );
}

/** How many cells fit the bar `ref` lands on, in the face it draws in,
 *  again as it resizes. None until it is measured. */
function useCells(ref: { current: HTMLSpanElement | null }): number {
  const [cells, setCells] = useState(0);
  useLayoutEffect(() => {
    const bar = ref.current;
    if (!bar) return;
    const measure = () => setCells(blockCells(bar.clientWidth, cellWidth(bar)));
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    observer.observe(bar);
    return () => observer.disconnect();
  }, [ref]);
  return cells;
}

let canvas: HTMLCanvasElement | null = null;

/** One full block in the face `bar` draws in, or 0 where nothing can
 *  measure. */
function cellWidth(bar: HTMLElement): number {
  canvas ??= document.createElement('canvas');
  const ctx = canvas.getContext?.('2d');
  if (!ctx) return 0;
  const style = getComputedStyle(bar);
  ctx.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
  return ctx.measureText(FULL_BLOCK).width;
}
