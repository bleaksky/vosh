import {
  useContext,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type RefObject,
} from 'react';
import { usePlayPalette } from '../../theme/fitGameColors';
import { roomNameColor, terrainLabel, type RoomNameGround } from './roomName';
import { groupPeople, useRoom, type RoomInfo, type RoomPerson } from '../../stores/gmcp/roomStore';
import { themeTokens, type XtermPalette } from '../../theme/themes';
import { useActiveTheme } from '../../theme/useActiveTheme';
import { MapView } from './MapView';
import { mapBandLayout, mapBandPeople, mapBandRows } from './mapBand';
import { PaneHeader, PaneMeta } from '../PaneHeader';
import { exitsLabel } from '../paneText';
import { PaneTextSizeContext } from '../paneTextSize';

// The Map pane: the server map drawing in a box inset 8 px
// with radius 8, then a band of dense rows for the room you stand in,
// a quiet row with its terrain, region and exits, and the people here.
// The room's name takes the theme color the terminal draws it in for
// its sector (roomName.ts). The band's height follows the pane alone,
// and the band is there from the first paint, so the drawing keeps its
// size as you walk and as people come and go. A crowded room counts the
// people past the last slot on that slot, and a short pane gives up
// people slots before the terrain row, and that row before the name.

export function MapPane() {
  const { info, people } = useRoom();
  const theme = useActiveTheme();
  const palette = usePlayPalette();
  const ground = useMemo(() => themeTokens(theme), [theme]);
  const boxRef = useRef<HTMLDivElement | null>(null);
  const rowsRef = useRef<HTMLUListElement | null>(null);
  const rows = useBandRows(boxRef, rowsRef);

  return (
    <>
      <PaneHeader meta={info?.area ? <PaneMeta>{info.area}</PaneMeta> : null} />
      <div ref={boxRef} className="pane-map-box">
        <MapView emptyText="The map appears when your MUD sends Map.Tiles." />
      </div>
      <ul
        ref={rowsRef}
        className="pane-rows pane-map-rows"
        style={{ '--band-rows': rows } as CSSProperties}
      >
        <MapBandRows info={info} people={people} rows={rows} palette={palette} ground={ground} />
      </ul>
    </>
  );
}

/** The band's rows for `rows` slots: the room's name, its terrain,
 *  region and exits, then the people here. The name and the terrain
 *  rows are there before the first Room.Info, empty, so the first room
 *  moves nothing. A band with no terrain row keeps the exits beside the
 *  name. */
export function MapBandRows({
  info,
  people,
  rows,
  palette,
  ground,
}: {
  info: RoomInfo | null;
  people: readonly RoomPerson[];
  rows: number;
  palette: XtermPalette;
  ground: RoomNameGround;
}) {
  const layout = mapBandLayout(rows);
  const { shown, rest } = mapBandPeople(groupPeople(people), layout.people);
  const others = rest.reduce((sum, g) => sum + g.count, 0);
  const color = info ? roomNameColor(info.sector, palette, ground) : null;
  const terrain = info ? terrainLabel(info.sector, info.terrain) : null;
  const region = info?.region ?? null;
  const exits = info && info.exits.length > 0 ? exitsLabel(info.exits) : null;

  return (
    <>
      <li className="pane-row pane-map-room">
        <span className="pane-row-name" title={info?.name} style={color ? { color } : undefined}>
          {info?.name}
        </span>
        {exits && !layout.where && (
          <span className="pane-row-value pane-map-exits" title={exits}>
            {exits}
          </span>
        )}
      </li>
      {layout.where && (
        <li className="pane-row pane-map-where">
          {terrain && <span className="pane-map-terrain">{terrain}</span>}
          {region && (
            <span className="pane-map-region" title={region}>
              {terrain && (
                <span className="pane-map-sep" aria-hidden="true">
                  ·
                </span>
              )}
              {region}
            </span>
          )}
          {exits && (
            <span className="pane-map-exits" title={exits}>
              {exits}
            </span>
          )}
        </li>
      )}
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
    </>
  );
}

/** How many rows the band holds, the room's rows included. The drawing
 *  and the band split what the pane leaves under its header, and only
 *  a new pane size or a new panel size changes that split, so the count
 *  never follows the rows it lays out. */
function useBandRows(
  boxRef: RefObject<HTMLDivElement | null>,
  rowsRef: RefObject<HTMLUListElement | null>,
): number {
  const [rows, setRows] = useState(1);
  // A new panel size makes every row taller or shorter in a pane that
  // may keep its size, so it measures again.
  const textSize = useContext(PaneTextSizeContext);
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
  }, [boxRef, rowsRef, textSize]);
  return rows;
}
