import { groupPeople, useRoom } from '../../lib/stores/roomStore';
import { ServerMapView } from '../ServerMapView';
import { PaneHeader, PaneMeta } from './PaneHeader';
import { exitsLabel } from './paneText';

// The Map pane (SPEC 9): the server map drawing in a box inset 8 px
// with radius 8, then one dense row for the room you stand in with its
// exits, then a row per person here. The drawing takes whatever height
// the rows leave, and a short pane gives up the people rows first.

export function MapPane() {
  const { info, people } = useRoom();
  const groups = groupPeople(people);

  return (
    <>
      <PaneHeader meta={info?.area ? <PaneMeta>{info.area}</PaneMeta> : null} />
      <div className="pane-map-box">
        <ServerMapView embedded emptyText="The map appears when your MUD sends Map.Tiles." />
      </div>
      {info && (
        <ul className="pane-rows pane-map-rows">
          <li className="pane-row">
            <span className="pane-row-name">{info.name}</span>
            {info.exits.length > 0 && (
              <span className="pane-row-value pane-map-exits">{exitsLabel(info.exits)}</span>
            )}
          </li>
          {groups.map((g) => (
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
