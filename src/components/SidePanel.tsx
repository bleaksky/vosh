import { MapPane } from './MapPane';

// Side panel now hosts only the map. Chat / Wealth / Group moved to
// the AuxDrawer (F2), and Affects collapses into the BottomHUD's
// tracked pills plus the drawer's full list. The pluggable
// sortable-panes machinery is gone — there's only one pane.
export function SidePanel() {
  return (
    <aside className="side-panel" aria-label="side panel">
      <div className="sidebar-section" data-panel-id="map-pane">
        <MapPane />
      </div>
    </aside>
  );
}
