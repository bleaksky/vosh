import { ServerMapView } from './ServerMapView';

// Embedded right-side map pane. Hosts the read-only server-map
// renderer with a single 1px divider on its left edge separating it
// from the terminal area. ServerMapView owns the pane's one header
// row — the quiet Ember caps label ("map · <area>") plus a sliders
// toggle — and keeps the mode / zoom / radius controls collapsed
// behind that toggle, so this wrapper stays pure layout.
export function MapPane() {
  return (
    <div className="map-pane">
      <div className="map-pane-body">
        <ServerMapView />
      </div>
    </div>
  );
}
