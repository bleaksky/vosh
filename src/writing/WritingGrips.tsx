import { useRef } from 'react';
import { saveWritingCardPrefs } from '../stores/config/writingCardStore';
import { BOX_ROWS_MIN, dragCols, dragRows } from './cardPlace';

// The writing card's two size grips, which keep what you drag in the
// card's prefs.

/** Sets the rows a drag shows before it lands. */
type SetRows = (rows: number | null) => void;

/** The grip on the box's free edge, which sizes its rows. `edge` is
 *  the edge it sits on. */
export function RowGrip({
  edge,
  boxRows,
  fit,
  lineH,
  setSizing,
}: {
  edge: 'foot' | 'top';
  boxRows: number;
  fit: number;
  lineH: number;
  setSizing: SetRows;
}) {
  const sizeRef = useRef<{ y: number; rows: number; to: number | null } | null>(null);
  const endSize = () => {
    const d = sizeRef.current;
    sizeRef.current = null;
    document.body.style.cursor = '';
    if (d?.to !== null && d?.to !== undefined) {
      void saveWritingCardPrefs({ rows: d.to }).catch(() => {});
    }
    setSizing(null);
  };
  return (
    <div
      className={`wr-grip is-${edge}`}
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize the text box"
      aria-valuemin={BOX_ROWS_MIN}
      aria-valuemax={Math.max(BOX_ROWS_MIN, fit)}
      aria-valuenow={boxRows}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        sizeRef.current = { y: e.clientY, rows: boxRows, to: null };
        document.body.style.cursor = 'ns-resize';
      }}
      onPointerMove={(e) => {
        const d = sizeRef.current;
        if (!d) return;
        d.to = dragRows(d.rows, e.clientY - d.y, lineH, fit, edge === 'foot' ? 1 : -1);
        setSizing(d.to);
      }}
      onPointerUp={endSize}
      onPointerCancel={endSize}
      // A double click lets the box grow with the text again.
      onDoubleClick={() => void saveWritingCardPrefs({ rows: null }).catch(() => {})}
    />
  );
}

// The grip in the text box's corner sizes the box both ways, as a text
// area's does: down for more rows and right for more columns, from 6
// rows and 75 columns up to what the window allows, and the card grows
// with it. A narrow window keeps 80 columns in a smaller face, so there
// the grip sizes the rows alone. A double click lets the box grow with
// the text again at 80 columns.
export function BoxGrip({
  boxRows,
  boxCols,
  fit,
  colsFit,
  lineH,
  colW,
  narrow,
  setSizing,
  setSizingCols,
}: {
  boxRows: number;
  boxCols: number;
  fit: number;
  colsFit: number;
  lineH: number;
  colW: number;
  narrow: boolean;
  setSizing: SetRows;
  setSizingCols: (cols: number | null) => void;
}) {
  const boxSizeRef = useRef<{
    x: number;
    y: number;
    rows: number;
    cols: number;
    toRows: number;
    toCols: number;
  } | null>(null);
  const endBoxSize = () => {
    const d = boxSizeRef.current;
    boxSizeRef.current = null;
    document.body.style.cursor = '';
    if (d) {
      const patch = {
        ...(d.toRows !== d.rows ? { rows: d.toRows } : {}),
        ...(d.toCols !== d.cols ? { cols: d.toCols } : {}),
      };
      if (Object.keys(patch).length > 0) void saveWritingCardPrefs(patch).catch(() => {});
    }
    setSizing(null);
    setSizingCols(null);
  };
  return (
    <div
      className="wr-box-grip"
      role="button"
      aria-label="Resize the text box"
      title="Drag to resize the text box. Double click to reset it."
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.stopPropagation();
        e.currentTarget.setPointerCapture(e.pointerId);
        boxSizeRef.current = {
          x: e.clientX,
          y: e.clientY,
          rows: boxRows,
          cols: boxCols,
          toRows: boxRows,
          toCols: boxCols,
        };
        document.body.style.cursor = narrow ? 'ns-resize' : 'nwse-resize';
      }}
      onPointerMove={(e) => {
        const d = boxSizeRef.current;
        if (!d) return;
        d.toRows = dragRows(d.rows, e.clientY - d.y, lineH, fit, 1);
        d.toCols = narrow ? d.cols : dragCols(d.cols, e.clientX - d.x, colW, colsFit);
        setSizing(d.toRows);
        setSizingCols(d.toCols);
      }}
      onPointerUp={endBoxSize}
      onPointerCancel={endBoxSize}
      onDoubleClick={() => void saveWritingCardPrefs({ rows: null, cols: null }).catch(() => {})}
    >
      <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">
        <path
          d="M11.5 4.5l-7 7M11.5 8.5l-3 3"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.2"
          strokeLinecap="round"
        />
      </svg>
    </div>
  );
}
