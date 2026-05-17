import { ServerMapView } from './ServerMapView';

// Embedded right-side map pane. Hosts the read-only server-map
// renderer with a thin in-pane header so the map reads as a
// contained region rather than a floating widget. Visually sits
// inside the main window — shares background + font + accent —
// with a single 1px divider on its left edge separating it from
// the terminal area.
export function MapPane() {
  return (
    <div className="map-pane">
      <div className="map-pane-header">
        <span className="map-pane-title">map</span>
      </div>
      <div className="map-pane-body">
        <ServerMapView />
      </div>
    </div>
  );
}
