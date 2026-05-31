import { ServerMapView } from './ServerMapView';

// Embedded right-side map pane. Hosts the read-only server-map
// renderer with a single 1px divider on its left edge separating it
// from the terminal area. The dedicated header bar that used to live
// here ("map" label) was folded into ServerMapView's existing
// subhead row so the pane gives back one row of vertical space and
// the title sits next to the radius / mode / zoom controls it
// labels — not as its own redundant strip.
export function MapPane() {
  return (
    <div className="map-pane">
      <div className="map-pane-body">
        <ServerMapView />
      </div>
    </div>
  );
}
