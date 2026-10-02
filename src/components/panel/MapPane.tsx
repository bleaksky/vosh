import { useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from 'react';
import { groupPeople, useRoom } from '../../lib/stores/roomStore';
import { ServerMapView } from '../ServerMapView';
import { mapBandPeople, mapBandRows } from './mapBand';
import { PaneHeader, PaneMeta } from './PaneHeader';
import { exitsLabel } from './paneText';

// The Map pane (SPEC 9): the server map drawing in a box inset 8 px
// with radius 8, then a band of dense rows for the room you stand in
// with its exits and the people here. The band's height follows the
// pane alone, and the band is there from the first paint, so the
// drawing keeps its size as you walk and as people come and go. A
// crowded room counts the people past the last slot on that slot, and
// a short pane gives up people slots before the room.

export function MapPane() {
  const { info, people } = useRoom();
  const boxRef = useRef<HTMLDivElement | null>(null);
  const rowsRef = useRef<HTMLUListElement | null>(null);
  const rows = useBandRows(boxRef, rowsRef);
  const { shown, rest } = mapBandPeople(groupPeople(people), rows - 1);
  const others = rest.reduce((sum, g) => sum + g.count, 0);

  return (
    <>
      <PaneHeader meta={info?.area ? <PaneMeta>{info.area}</PaneMeta> : null} />
      <div ref={boxRef} className="pane-map-box">
        <ServerMapView emptyText="The map appears when your MUD sends Map.Tiles." />
      </div>
      <ul
        ref={rowsRef}
        className="pane-rows pane-map-rows"
        style={{ '--band-rows': rows } as CSSProperties}
      >
        <li className="pane-row">
          <span className="pane-row-name">{info?.name}</span>
          {info && info.exits.length > 0 && (
            <span className="pane-row-value pane-map-exits">{exitsLabel(info.exits)}</span>
          )}
        </li>
        {shown.map((g) => (
          <li key={g.name} className="pane-row pane-map-person">
            <span className="pane-row-name">{g.name}</span>
            {g.count > 1 && <span className="pane-row-value pane-map-count">{g.count}</span>}
          </li>
        ))}
        {others > 0 && (
          <li className="pane-row pane-map-more">
            <span className="pane-row-name">{others} others here</span>
          </li>
        )}
      </ul>
    </>
  );
}

/** How many rows the band holds, the room row included. The drawing
 *  and the band split what the pane leaves under its header, and only
 *  a new pane size changes that split, so the count never follows the
 *  rows it lays out. */
function useBandRows(
  boxRef: RefObject<HTMLDivElement | null>,
  rowsRef: RefObject<HTMLUListElement | null>,
): number {
  const [rows, setRows] = useState(1);
  useLayoutEffect(() => {
    const box = boxRef.current;
    const pane = box?.parentElement;
    if (!box || !pane) return;
    const measure = () => {
      const band = rowsRef.current;
      if (!band) return;
      const row = (band.firstElementChild as HTMLElement | null)?.offsetHeight || 22;
      const floor = parseFloat(getComputedStyle(box).minHeight) || 0;
      const shared = box.offsetHeight + band.offsetHeight;
      const next = mapBandRows(shared, floor, row);
      setRows((prev) => (prev === next ? prev : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(pane);
    return () => observer.disconnect();
  }, [boxRef, rowsRef]);
  return rows;
}
