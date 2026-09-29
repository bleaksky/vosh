import { useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { groupPeople, useRoom } from '../../lib/stores/roomStore';
import { ServerMapView } from '../ServerMapView';
import { PaneHeader, PaneMeta } from './PaneHeader';
import { exitsLabel } from './paneText';

// The Map pane (SPEC 9): the server map drawing in a box inset 8 px
// with radius 8, then one dense row for the room you stand in with its
// exits, then a row per person here. The drawing takes whatever height
// the rows leave down to its floor. Below that a short pane gives up
// the people rows, whole rows from the bottom, and keeps the room row.

export function MapPane() {
  const { info, people } = useRoom();
  const groups = groupPeople(people);
  const boxRef = useRef<HTMLDivElement | null>(null);
  const rowsRef = useRef<HTMLUListElement | null>(null);
  const fit = useRowsThatFit(boxRef, rowsRef, info !== null);
  const shown = fit === null ? groups : groups.slice(0, Math.max(0, fit - 1));

  return (
    <>
      <PaneHeader meta={info?.area ? <PaneMeta>{info.area}</PaneMeta> : null} />
      <div ref={boxRef} className="pane-map-box">
        <ServerMapView embedded emptyText="The map appears when your MUD sends Map.Tiles." />
      </div>
      {info && (
        <ul ref={rowsRef} className="pane-rows pane-map-rows">
          <li className="pane-row">
            <span className="pane-row-name">{info.name}</span>
            {info.exits.length > 0 && (
              <span className="pane-row-value pane-map-exits">{exitsLabel(info.exits)}</span>
            )}
          </li>
          {shown.map((g) => (
            <li key={g.name} className="pane-row pane-map-person">
              <span className="pane-row-name">{g.name}</span>
              {g.count > 1 && <span className="pane-row-value pane-map-count">{g.count}</span>}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

/** How many dense rows, the room row included, fit under the drawing.
 *  The rows may take what they hold now plus whatever the drawing
 *  holds above its floor. Null until measured. Watches the pane rather
 *  than the rows, so dropping a row never resizes what it watches. */
function useRowsThatFit(
  boxRef: RefObject<HTMLDivElement | null>,
  rowsRef: RefObject<HTMLUListElement | null>,
  hasRows: boolean,
): number | null {
  const [fit, setFit] = useState<number | null>(null);
  useLayoutEffect(() => {
    const box = boxRef.current;
    const pane = box?.parentElement;
    if (!box || !pane || !hasRows) return;
    const measure = () => {
      const rows = rowsRef.current;
      if (!rows) return;
      const row = (rows.firstElementChild as HTMLElement | null)?.offsetHeight || 22;
      const floor = parseFloat(getComputedStyle(box).minHeight) || 0;
      const room = rows.clientHeight + Math.max(0, box.clientHeight - floor);
      const next = Math.max(1, Math.floor((room + 0.5) / row));
      setFit((prev) => (prev === next ? prev : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(pane);
    return () => observer.disconnect();
  }, [boxRef, rowsRef, hasRows]);
  return fit;
}
