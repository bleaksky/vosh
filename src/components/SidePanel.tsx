import { StatusPane } from './StatusPane';
import { MapPane } from './MapPane';
import { AffectsPane } from './AffectsPane';
import { InfoTabsPane } from './InfoTabsPane';

export function SidePanel() {
  return (
    <aside className="side-panel" aria-label="side panel">
      <StatusPane />
      <AffectsPane />
      <MapPane />
      <InfoTabsPane />
    </aside>
  );
}
