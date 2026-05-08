import { StatusPane } from './StatusPane';
import { MapPane } from './MapPane';
import { ChatPane } from './ChatPane';
import { AffectsPane } from './AffectsPane';

export function SidePanel() {
  return (
    <aside className="side-panel" aria-label="side panel">
      <StatusPane />
      <AffectsPane />
      <MapPane />
      <ChatPane />
    </aside>
  );
}
