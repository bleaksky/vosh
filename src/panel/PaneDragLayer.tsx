import { usePaneDrag } from './paneDrag';

// What a pane drag draws over the window: where the pane would land,
// washed in the accent with a 2 px line along the edge it joins, and a
// ghost of its name beside the pointer. Nothing while no drag runs.

export function PaneDragLayer() {
  const drag = usePaneDrag();
  if (!drag) return null;
  const { target } = drag;
  return (
    <div className="pane-drag-layer" aria-hidden="true">
      {target && (
        <>
          <div
            className="pane-drop-wash"
            style={{
              left: target.wash.x,
              top: target.wash.y,
              width: target.wash.w,
              height: target.wash.h,
            }}
          />
          <div
            className="pane-drop-line"
            style={{ left: target.line.x, top: target.line.y, width: target.line.w }}
          />
        </>
      )}
      <div className="pane-drag-ghost" style={{ left: drag.x + 14, top: drag.y + 16 }}>
        <span className="pane-label">{drag.label}</span>
      </div>
    </div>
  );
}
